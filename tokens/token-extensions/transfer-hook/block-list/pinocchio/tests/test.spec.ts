import { Buffer } from "node:buffer";
import * as path from "node:path";
import {
  ASSOCIATED_TOKEN_PROGRAM_ID,
  createAssociatedTokenAccountInstruction,
  createInitializeMintInstruction,
  createInitializeTransferHookInstruction,
  createMintToCheckedInstruction,
  createTransferCheckedInstruction,
  ExtensionType,
  getAssociatedTokenAddressSync,
  getMintLen,
  TOKEN_2022_PROGRAM_ID,
} from "@solana/spl-token";
import {
  ComputeBudgetProgram,
  Keypair,
  PublicKey,
  SystemProgram,
  Transaction,
  TransactionInstruction,
} from "@solana/web3.js";
import { assert } from "chai";
import { FailedTransactionMetadata, LiteSVM, type TransactionMetadata } from "litesvm";
import { before, describe, it } from "mocha";

// Program ID baked into the on-chain program (`declare_id!` in program/src/lib.rs).
const BLOCK_LIST_PROGRAM_ID = new PublicKey("BLoCKLSG2qMQ9YxEyrrKKAQzthvW4Lu8Eyv74axF6mf");
const PROGRAM_SO_PATH = path.resolve(__dirname, "fixtures/block_list.so");

// Instruction discriminators from program/src/instructions/*.rs.
const INIT_DISCRIMINATOR = 0xf1;
const BLOCK_WALLET_DISCRIMINATOR = 0xf2;
const UNBLOCK_WALLET_DISCRIMINATOR = 0xf3;
const SETUP_EXTRA_METAS_DISCRIMINATOR = 0x6a;

function findConfigPda(): PublicKey {
  return PublicKey.findProgramAddressSync([Buffer.from("config")], BLOCK_LIST_PROGRAM_ID)[0];
}

function findWalletBlockPda(wallet: PublicKey): PublicKey {
  return PublicKey.findProgramAddressSync([Buffer.from("wallet_block"), wallet.toBuffer()], BLOCK_LIST_PROGRAM_ID)[0];
}

function findExtraMetasPda(mint: PublicKey): PublicKey {
  return PublicKey.findProgramAddressSync(
    [Buffer.from("extra-account-metas"), mint.toBuffer()],
    BLOCK_LIST_PROGRAM_ID,
  )[0];
}

function buildInitIx(authority: PublicKey): TransactionInstruction {
  return new TransactionInstruction({
    programId: BLOCK_LIST_PROGRAM_ID,
    keys: [
      { pubkey: authority, isSigner: true, isWritable: true },
      { pubkey: findConfigPda(), isSigner: false, isWritable: true },
      { pubkey: SystemProgram.programId, isSigner: false, isWritable: false },
    ],
    data: Buffer.from([INIT_DISCRIMINATOR]),
  });
}

function buildBlockWalletIx(authority: PublicKey, wallet: PublicKey): TransactionInstruction {
  return new TransactionInstruction({
    programId: BLOCK_LIST_PROGRAM_ID,
    keys: [
      { pubkey: authority, isSigner: true, isWritable: true },
      { pubkey: findConfigPda(), isSigner: false, isWritable: true },
      { pubkey: wallet, isSigner: false, isWritable: false },
      { pubkey: findWalletBlockPda(wallet), isSigner: false, isWritable: true },
      { pubkey: SystemProgram.programId, isSigner: false, isWritable: false },
    ],
    data: Buffer.from([BLOCK_WALLET_DISCRIMINATOR]),
  });
}

function buildUnblockWalletIx(authority: PublicKey, wallet: PublicKey): TransactionInstruction {
  return new TransactionInstruction({
    programId: BLOCK_LIST_PROGRAM_ID,
    keys: [
      { pubkey: authority, isSigner: true, isWritable: true },
      { pubkey: findConfigPda(), isSigner: false, isWritable: true },
      { pubkey: findWalletBlockPda(wallet), isSigner: false, isWritable: true },
      { pubkey: SystemProgram.programId, isSigner: false, isWritable: false },
    ],
    data: Buffer.from([UNBLOCK_WALLET_DISCRIMINATOR]),
  });
}

function buildSetupExtraMetasIx(
  authority: PublicKey,
  mint: PublicKey,
  checkBothWallets: boolean,
): TransactionInstruction {
  // Second byte is the optional `checkBothWallets` flag read by the program
  // when blocked_wallets_count > 0 (see program/src/instructions/setup_extra_metas.rs).
  const data = Buffer.from([SETUP_EXTRA_METAS_DISCRIMINATOR, checkBothWallets ? 1 : 0]);
  return new TransactionInstruction({
    programId: BLOCK_LIST_PROGRAM_ID,
    keys: [
      { pubkey: authority, isSigner: true, isWritable: true },
      { pubkey: findConfigPda(), isSigner: false, isWritable: false },
      { pubkey: mint, isSigner: false, isWritable: false },
      { pubkey: findExtraMetasPda(mint), isSigner: false, isWritable: true },
      { pubkey: SystemProgram.programId, isSigner: false, isWritable: false },
    ],
    data,
  });
}

function buildTransferIxWithHookAccounts(args: {
  source: PublicKey;
  mint: PublicKey;
  destination: PublicKey;
  owner: PublicKey;
  amount: bigint;
  decimals: number;
  sourceOwner: PublicKey;
  destinationOwner: PublicKey;
  extraMode: "empty" | "source-only" | "both";
}): TransactionInstruction {
  const baseIx = createTransferCheckedInstruction(
    args.source,
    args.mint,
    args.destination,
    args.owner,
    args.amount,
    args.decimals,
    [],
    TOKEN_2022_PROGRAM_ID,
  );

  const extraKeys: { pubkey: PublicKey; isSigner: boolean; isWritable: boolean }[] = [];
  if (args.extraMode === "source-only" || args.extraMode === "both") {
    extraKeys.push({
      pubkey: findWalletBlockPda(args.sourceOwner),
      isSigner: false,
      isWritable: false,
    });
  }
  if (args.extraMode === "both") {
    extraKeys.push({
      pubkey: findWalletBlockPda(args.destinationOwner),
      isSigner: false,
      isWritable: false,
    });
  }

  // Token Extensions invokes the hook with these trailing accounts in this order:
  //   [4] validation_pda (extra-account-metas)
  //   [5] resolved wallet_block for the source TA (when present)
  //   [6] resolved wallet_block for the destination TA (when present)
  // The hook program also needs the hook program id to be addressable so the
  // Token Extensions transfer instruction handler can CPI into it; we append
  // that at the very end (Token Extensions strips it from the hook accounts list).
  return new TransactionInstruction({
    programId: baseIx.programId,
    data: baseIx.data,
    keys: [
      ...baseIx.keys,
      { pubkey: findExtraMetasPda(args.mint), isSigner: false, isWritable: false },
      ...extraKeys,
      { pubkey: BLOCK_LIST_PROGRAM_ID, isSigner: false, isWritable: false },
    ],
  });
}

function expectTxOk(svm: LiteSVM, tx: Transaction, signers: Keypair[], label: string): TransactionMetadata {
  // litesvm reuses blockhashes; without expiring, identical txs across tests
  // (same signers, same ix, same blockhash) collide on signature and are
  // rejected as `AlreadyProcessed`. We expire before every send so each tx
  // gets a fresh blockhash and a unique signature.
  svm.expireBlockhash();
  tx.recentBlockhash = svm.latestBlockhash();
  tx.sign(...signers);
  const res = svm.sendTransaction(tx);
  if (res instanceof FailedTransactionMetadata) {
    const logs = res.meta().logs().join("\n");
    throw new Error(`${label} failed: ${res.err()}\nlogs:\n${logs}`);
  }
  return res;
}

function expectTxFails(svm: LiteSVM, tx: Transaction, signers: Keypair[], label: string): string[] {
  svm.expireBlockhash();
  tx.recentBlockhash = svm.latestBlockhash();
  tx.sign(...signers);
  const res = svm.sendTransaction(tx);
  if (!(res instanceof FailedTransactionMetadata)) {
    throw new Error(`${label} unexpectedly succeeded`);
  }
  return res.meta().logs();
}

describe("block-list pinocchio transfer-hook", () => {
  let svm: LiteSVM;
  let payer: Keypair;
  let mintKeypair: Keypair;
  let walletA: Keypair;
  let walletB: Keypair;
  let ataA: PublicKey;
  let ataB: PublicKey;

  const DECIMALS = 6;
  const MINT_AMOUNT = 1_000n * 10n ** BigInt(DECIMALS);
  const TRANSFER_AMOUNT = 10n * 10n ** BigInt(DECIMALS);

  before(() => {
    svm = new LiteSVM();
    svm.addProgramFromFile(BLOCK_LIST_PROGRAM_ID, PROGRAM_SO_PATH);
    payer = Keypair.generate();
    walletA = Keypair.generate();
    walletB = Keypair.generate();
    mintKeypair = Keypair.generate();
    svm.airdrop(payer.publicKey, 1_000_000_000n);
    svm.airdrop(walletA.publicKey, 100_000_000n);
  });

  it("init: creates the config PDA", () => {
    const tx = new Transaction().add(buildInitIx(payer.publicKey));
    expectTxOk(svm, tx, [payer], "init");

    const config = svm.getAccount(findConfigPda());
    assert.isNotNull(config, "config PDA should exist after init");
    // Layout: discriminator(1) | authority(32) | blocked_wallets_count(8).
    assert.strictEqual(config!.data.length, 41);
    assert.strictEqual(config!.data[0], 0x01, "config discriminator");
    assert.strictEqual(
      new PublicKey(config!.data.slice(1, 33)).toBase58(),
      payer.publicKey.toBase58(),
      "config authority",
    );
    const view = new DataView(config!.data.buffer, config!.data.byteOffset + 33, 8);
    assert.strictEqual(view.getBigUint64(0, true), 0n, "blocked_wallets_count starts at 0");
  });

  it("creates a Token Extensions mint with TransferHook -> block-list, plus extra metas", () => {
    const mintLen = getMintLen([ExtensionType.TransferHook]);
    const mintRent = svm.minimumBalanceForRentExemption(BigInt(mintLen));

    const createMintAccountIx = SystemProgram.createAccount({
      fromPubkey: payer.publicKey,
      newAccountPubkey: mintKeypair.publicKey,
      lamports: Number(mintRent),
      space: mintLen,
      programId: TOKEN_2022_PROGRAM_ID,
    });
    const initHookIx = createInitializeTransferHookInstruction(
      mintKeypair.publicKey,
      payer.publicKey,
      BLOCK_LIST_PROGRAM_ID,
      TOKEN_2022_PROGRAM_ID,
    );
    const initMintIx = createInitializeMintInstruction(
      mintKeypair.publicKey,
      DECIMALS,
      payer.publicKey,
      null,
      TOKEN_2022_PROGRAM_ID,
    );

    const tx = new Transaction().add(createMintAccountIx, initHookIx, initMintIx);
    expectTxOk(svm, tx, [payer, mintKeypair], "create-mint");

    // Setup the extra-metas account. With 0 blocked wallets this writes the
    // EMPTY ExtraAccountMetaList shape.
    const setupTx = new Transaction().add(buildSetupExtraMetasIx(payer.publicKey, mintKeypair.publicKey, false));
    expectTxOk(svm, setupTx, [payer], "setup_extra_metas (empty)");

    const extraMetas = svm.getAccount(findExtraMetasPda(mintKeypair.publicKey));
    assert.isNotNull(extraMetas, "extra-metas PDA exists");
    // Empty ExtraAccountMetaList = 8 byte TLV header + 4 bytes length + 4 bytes count = 16 bytes.
    assert.strictEqual(extraMetas!.data.length, 16, "empty extra-metas data length");
  });

  it("creates ATAs with the ImmutableOwner extension and mints to wallet A", () => {
    ataA = getAssociatedTokenAddressSync(
      mintKeypair.publicKey,
      walletA.publicKey,
      false,
      TOKEN_2022_PROGRAM_ID,
      ASSOCIATED_TOKEN_PROGRAM_ID,
    );
    ataB = getAssociatedTokenAddressSync(
      mintKeypair.publicKey,
      walletB.publicKey,
      false,
      TOKEN_2022_PROGRAM_ID,
      ASSOCIATED_TOKEN_PROGRAM_ID,
    );

    const createA = createAssociatedTokenAccountInstruction(
      payer.publicKey,
      ataA,
      walletA.publicKey,
      mintKeypair.publicKey,
      TOKEN_2022_PROGRAM_ID,
      ASSOCIATED_TOKEN_PROGRAM_ID,
    );
    const createB = createAssociatedTokenAccountInstruction(
      payer.publicKey,
      ataB,
      walletB.publicKey,
      mintKeypair.publicKey,
      TOKEN_2022_PROGRAM_ID,
      ASSOCIATED_TOKEN_PROGRAM_ID,
    );
    const mintToA = createMintToCheckedInstruction(
      mintKeypair.publicKey,
      ataA,
      payer.publicKey,
      MINT_AMOUNT,
      DECIMALS,
      [],
      TOKEN_2022_PROGRAM_ID,
    );

    expectTxOk(svm, new Transaction().add(createA, createB, mintToA), [payer], "create-atas+mint");

    const ataAData = svm.getAccount(ataA)!.data;
    assert.isAbove(ataAData.length, 165, "ATA has extension data (immutable owner)");
  });

  it("transfer succeeds when source wallet is not blocked", () => {
    const tx = new Transaction().add(
      ComputeBudgetProgram.setComputeUnitLimit({ units: 400_000 }),
      buildTransferIxWithHookAccounts({
        source: ataA,
        mint: mintKeypair.publicKey,
        destination: ataB,
        owner: walletA.publicKey,
        amount: TRANSFER_AMOUNT,
        decimals: DECIMALS,
        sourceOwner: walletA.publicKey,
        destinationOwner: walletB.publicKey,
        extraMode: "empty",
      }),
    );
    expectTxOk(svm, tx, [walletA], "transfer (unblocked)");
  });

  it("block_wallet: blocks wallet A and bumps blocked_wallets_count", () => {
    const tx = new Transaction().add(buildBlockWalletIx(payer.publicKey, walletA.publicKey));
    expectTxOk(svm, tx, [payer], "block_wallet A");

    const wb = svm.getAccount(findWalletBlockPda(walletA.publicKey));
    assert.isNotNull(wb, "wallet_block PDA created");
    assert.strictEqual(wb!.data[0], 0x02, "wallet_block discriminator");

    const config = svm.getAccount(findConfigPda())!;
    const view = new DataView(config.data.buffer, config.data.byteOffset + 33, 8);
    assert.strictEqual(view.getBigUint64(0, true), 1n, "blocked_wallets_count == 1");
  });

  it("transfer from blocked source wallet fails with AccountBlocked", () => {
    const setupTx = new Transaction().add(buildSetupExtraMetasIx(payer.publicKey, mintKeypair.publicKey, false));
    expectTxOk(svm, setupTx, [payer], "setup_extra_metas (source dep)");

    const extraMetas = svm.getAccount(findExtraMetasPda(mintKeypair.publicKey))!;
    // 16-byte header + 35 bytes per ExtraAccountMeta entry = 51.
    assert.strictEqual(extraMetas.data.length, 51, "source-dependency extra-metas data length");

    const tx = new Transaction().add(
      ComputeBudgetProgram.setComputeUnitLimit({ units: 400_000 }),
      buildTransferIxWithHookAccounts({
        source: ataA,
        mint: mintKeypair.publicKey,
        destination: ataB,
        owner: walletA.publicKey,
        amount: TRANSFER_AMOUNT,
        decimals: DECIMALS,
        sourceOwner: walletA.publicKey,
        destinationOwner: walletB.publicKey,
        extraMode: "source-only",
      }),
    );
    const logs = expectTxFails(svm, tx, [walletA], "transfer-from-blocked");
    const joined = logs.join("\n");
    // `BlockListError::AccountBlocked` is variant index 2 -> custom code 0x2.
    // The hook returns this when the source wallet has a wallet_block PDA.
    assert.match(
      joined,
      /custom program error: 0x2/,
      `expected AccountBlocked (custom 0x2) error in logs, got:\n${joined}`,
    );
  });

  it("unblock_wallet: unblocks wallet A, blocked_wallets_count decrements, transfers work again", () => {
    const tx = new Transaction().add(buildUnblockWalletIx(payer.publicKey, walletA.publicKey));
    expectTxOk(svm, tx, [payer], "unblock_wallet A");

    assert.isNull(svm.getAccount(findWalletBlockPda(walletA.publicKey)), "wallet_block PDA closed");

    const config = svm.getAccount(findConfigPda())!;
    const view = new DataView(config.data.buffer, config.data.byteOffset + 33, 8);
    assert.strictEqual(view.getBigUint64(0, true), 0n, "blocked_wallets_count back to 0");

    // Re-issue the transfer with the (now-closed) wallet_block PDA still in
    // the extra metas. After unblock the wallet_block account no longer
    // exists on-chain (lamports drained, data zeroed), so `data_is_empty()` is
    // true in the hook and the transfer is no longer blocked.
    const transferTx = new Transaction().add(
      ComputeBudgetProgram.setComputeUnitLimit({ units: 400_000 }),
      buildTransferIxWithHookAccounts({
        source: ataA,
        mint: mintKeypair.publicKey,
        destination: ataB,
        owner: walletA.publicKey,
        amount: TRANSFER_AMOUNT,
        decimals: DECIMALS,
        sourceOwner: walletA.publicKey,
        destinationOwner: walletB.publicKey,
        extraMode: "source-only",
      }),
    );
    expectTxOk(svm, transferTx, [walletA], "transfer (after unblock)");
  });
});
