// In-process integration test: the program `.so` is loaded into a LiteSVM
// instance (no validator) and driven through the web3.js instruction
// builders in ../ts.

import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { describe, test } from "node:test";
import { fileURLToPath } from "node:url";

import {
  AccountRole,
  type Address,
  appendTransactionMessageInstruction,
  createTransactionMessage,
  generateKeyPairSigner,
  type Instruction,
  lamports,
  pipe,
  setTransactionMessageFeePayerSigner,
  signTransactionMessageWithSigners,
} from "@solana/kit";
import { PublicKey, type TransactionInstruction } from "@solana/web3.js";
import { FailedTransactionMetadata, LiteSVM } from "litesvm";

import { createCloseUserInstruction, createCreateUserInstruction } from "../ts";

const here = dirname(fileURLToPath(import.meta.url));
const programSoPath = join(here, "fixtures", "close_account_native_program.so");

// LiteSVM's default fee: 5000 lamports per signature, one signer per
// transaction in these tests.
const TRANSACTION_FEE_LAMPORTS = 5000n;

/** Convert a web3.js TransactionInstruction (from ../ts) into a kit Instruction. */
function toKitInstruction(instruction: TransactionInstruction): Instruction {
  return {
    programAddress: instruction.programId.toBase58() as Address,
    accounts: instruction.keys.map((meta) => ({
      address: meta.pubkey.toBase58() as Address,
      role: meta.isSigner
        ? meta.isWritable
          ? AccountRole.WRITABLE_SIGNER
          : AccountRole.READONLY_SIGNER
        : meta.isWritable
          ? AccountRole.WRITABLE
          : AccountRole.READONLY,
    })),
    data: new Uint8Array(instruction.data),
  };
}

async function sendIx(
  svm: LiteSVM,
  feePayer: Awaited<ReturnType<typeof generateKeyPairSigner>>,
  instruction: Instruction,
) {
  const tx = await pipe(
    createTransactionMessage({ version: 0 }),
    (m) => setTransactionMessageFeePayerSigner(feePayer, m),
    (m) => svm.setTransactionMessageLifetimeUsingLatestBlockhash(m),
    (m) => appendTransactionMessageInstruction(instruction, m),
    (m) => signTransactionMessageWithSigners(m),
  );
  const result = svm.sendTransaction(tx);
  if (result instanceof FailedTransactionMetadata) {
    throw new Error(`Transaction failed: ${result.err()}\n${result.meta().logs().join("\n")}`);
  }
  return result;
}

describe("Close Account!", () => {
  test("create, reject a non-owner close, then close and recover every lamport", async () => {
    const svm = new LiteSVM();
    const programId = (await generateKeyPairSigner()).address;
    svm.addProgram(programId, readFileSync(programSoPath));

    const payer = await generateKeyPairSigner();
    svm.airdrop(payer.address, lamports(10_000_000_000n));
    const payerPublicKey = new PublicKey(payer.address);
    const programPublicKey = new PublicKey(programId);

    const userAccount = PublicKey.findProgramAddressSync(
      [Buffer.from("USER"), payerPublicKey.toBuffer()],
      programPublicKey,
    )[0];
    const userAccountAddress = userAccount.toBase58() as Address;

    // 1. Create the user account.
    await sendIx(
      svm,
      payer,
      toKitInstruction(createCreateUserInstruction(userAccount, payerPublicKey, programPublicKey, "Jacob")),
    );

    const userAccountLamports = svm.getBalance(userAccountAddress);
    assert.ok(
      userAccountLamports !== null && userAccountLamports > 0n,
      "user account should hold rent lamports after create",
    );

    // 2. A non-owner cannot close it: the attacker signs as payer, but the
    // target is the victim's PDA, so the program's seeds check rejects it.
    const attacker = await generateKeyPairSigner();
    svm.airdrop(attacker.address, lamports(1_000_000_000n));
    const attackerPublicKey = new PublicKey(attacker.address);

    await assert.rejects(
      sendIx(
        svm,
        attacker,
        toKitInstruction(createCloseUserInstruction(userAccount, attackerPublicKey, programPublicKey)),
      ),
      "closing someone else's account must fail",
    );

    // 3. Naming the victim as payer without their signature is rejected too.
    const closeWithoutSignature = toKitInstruction(
      createCloseUserInstruction(userAccount, payerPublicKey, programPublicKey),
    );
    const demotedAccounts = closeWithoutSignature.accounts!.map((meta) =>
      meta.address === payer.address ? { address: meta.address, role: AccountRole.WRITABLE } : meta,
    );
    await assert.rejects(
      sendIx(svm, attacker, { ...closeWithoutSignature, accounts: demotedAccounts }),
      "closing without the owner's signature must fail",
    );
    assert.equal(
      svm.getBalance(userAccountAddress),
      userAccountLamports,
      "victim account must survive the attacks untouched",
    );

    // 4. The owner closes it and recovers every lamport (minus the
    // transaction fee). Nothing is stranded at the PDA.
    const payerBalanceBefore = svm.getBalance(payer.address)!;
    await sendIx(
      svm,
      payer,
      toKitInstruction(createCloseUserInstruction(userAccount, payerPublicKey, programPublicKey)),
    );

    const payerBalanceAfter = svm.getBalance(payer.address)!;
    assert.equal(
      payerBalanceAfter,
      payerBalanceBefore + userAccountLamports - TRANSACTION_FEE_LAMPORTS,
      "payer should recover every lamport the PDA held",
    );

    const closedBalance = svm.getBalance(userAccountAddress);
    assert.ok(closedBalance === null || closedBalance === 0n, "closed account should hold no lamports");
  });
});
