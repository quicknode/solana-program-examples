import { createServer, type IncomingMessage, type Server } from "node:http";
import type { AddressInfo } from "node:net";
import {
  type Address,
  decompileTransactionMessage,
  getBase58Decoder,
  getBase58Encoder,
  getBase64Decoder,
  getBase64Encoder,
  getCompiledTransactionMessageDecoder,
  getPublicKeyFromAddress,
  getTransactionDecoder,
  type Instruction,
  type ReadonlyUint8Array,
  type SignatureBytes,
  verifySignature,
} from "@solana/kit";
import { findAssociatedTokenPda } from "@solana-program/token";
import { getMintDecoder, getTokenDecoder } from "@solana-program/token-2022";
import { type WebSocket, WebSocketServer } from "ws";
import {
  ESCROW_PROGRAM_ADDRESS,
  EscrowInstruction,
  findOfferPda,
  getOfferDecoder,
  getOfferEncoder,
  identifyEscrowInstruction,
  parseCancelOfferInstruction,
  parseMakeOfferInstruction,
  parseTakeOfferInstruction,
} from "../../src/generated";
import type { FakeAccount } from "../fake-rpc";
import { tokenAccount } from "../fixtures";

// A Solana cluster small enough to hold in a test: an in-memory account
// store behind the JSON-RPC and RPC Subscriptions methods the app calls.
//
// Transactions are decoded, their signatures verified, and each escrow
// instruction checked against the account constraints the program declares:
// the offer at its PDA, every token account the associated token account of
// the owner and mint the constraints name, the maker and mints the ones the
// offer stored, the taker's bounds, and the amounts. An instruction that
// passes has the effect the program's handler has. A wrong account fails the
// transaction with the error code Anchor would raise, so a frontend that
// derives an account wrongly fails here as it would against the program.
//
// This checks the frontend against the program's interface. The program's own
// behavior is checked by its Rust tests, against the real compiled program.

const COMPUTE_BUDGET_PROGRAM = "ComputeBudget111111111111111111111111111111";
const SYSTEM_PROGRAM = "11111111111111111111111111111111";
const UPGRADEABLE_LOADER = "BPFLoaderUpgradeab1e11111111111111111111111";

type StoredAccount = FakeAccount & { lamports: bigint; executable?: boolean };

class ProgramError extends Error {
  constructor(
    readonly code: number,
    message: string,
  ) {
    super(message);
  }
}

const rentFor = (size: number) => BigInt((size + 128) * 6960);

export type MockCluster = {
  rpcUrl: string;
  rpcSubscriptionsUrl: string;
  accounts: Map<Address, StoredAccount>;
  // Every transaction the cluster accepted, newest last.
  transactions: Array<{ signature: string; instructions: Array<Instruction> }>;
  close: () => Promise<void>;
};

export const startMockCluster = async (
  initialAccounts: Map<Address, FakeAccount & { lamports?: bigint }>,
  port = 0,
): Promise<MockCluster> => {
  const accounts = new Map<Address, StoredAccount>();
  for (const [address, account] of initialAccounts) {
    accounts.set(address, { ...account, lamports: account.lamports ?? rentFor(account.data.length) });
  }
  accounts.set(ESCROW_PROGRAM_ADDRESS, {
    owner: UPGRADEABLE_LOADER as Address,
    data: new Uint8Array(36),
    lamports: 1_000_000n,
    executable: true,
  });

  const transactions: MockCluster["transactions"] = [];
  const processedSignatures = new Set<string>();
  let slot = 100n;

  const subscriptions = new Map<number, { socket: WebSocket; method: string; params: Array<unknown> }>();
  let nextSubscriptionId = 1;

  const encodeAccount = (account: StoredAccount | undefined) =>
    account === undefined
      ? null
      : {
          owner: account.owner,
          data: [getBase64Decoder().decode(account.data), "base64"],
          lamports: Number(account.lamports),
          executable: account.executable ?? false,
          rentEpoch: 0,
          space: account.data.length,
        };

  const context = () => ({ slot: Number(slot) });

  const notifyProgramSubscribers = (changed: Array<Address>) => {
    for (const [id, subscription] of subscriptions) {
      if (subscription.method !== "programSubscribe") {
        continue;
      }
      for (const address of changed) {
        const account = accounts.get(address);
        subscription.socket.send(
          JSON.stringify({
            jsonrpc: "2.0",
            method: "programNotification",
            params: {
              subscription: id,
              result: {
                context: context(),
                value: {
                  pubkey: address,
                  account: encodeAccount(account) ?? {
                    owner: SYSTEM_PROGRAM,
                    data: ["", "base64"],
                    lamports: 0,
                    executable: false,
                    rentEpoch: 0,
                    space: 0,
                  },
                },
              },
            },
          }),
        );
      }
    }
  };

  // Token helpers over the store.

  const readToken = (address: Address) => {
    const account = accounts.get(address);
    return account ? getTokenDecoder().decode(account.data) : null;
  };

  const writeTokenAmount = (address: Address, mint: Address, owner: Address, program: Address, amount: bigint) => {
    const fresh = tokenAccount(mint, owner, amount, program);
    const existing = accounts.get(address);
    accounts.set(address, { ...fresh, lamports: existing?.lamports ?? rentFor(fresh.data.length) });
  };

  const transfer = (from: Address, to: Address, amount: bigint) => {
    const source = readToken(from);
    const destination = readToken(to);
    if (!source || !destination) {
      throw new ProgramError(3012, `Token account ${source ? to : from} does not exist`);
    }
    if (source.amount < amount) {
      throw new ProgramError(1, "Error: insufficient funds");
    }
    const program = accounts.get(from)?.owner as Address;
    writeTokenAmount(from, source.mint, source.owner, program, source.amount - amount);
    writeTokenAmount(to, destination.mint, destination.owner, program, destination.amount + amount);
  };

  const requireAssociated = async (
    actual: Address,
    owner: Address,
    mint: Address,
    tokenProgram: Address,
    name: string,
  ) => {
    const [expected] = await findAssociatedTokenPda({ owner, mint, tokenProgram });
    if (actual !== expected) {
      // Anchor's ConstraintAssociated.
      throw new ProgramError(2009, `${name} is not the associated token account of ${owner} for ${mint}`);
    }
  };

  const requireMint = (mint: Address, tokenProgram: Address) => {
    const account = accounts.get(mint);
    if (!account || account.owner !== tokenProgram) {
      throw new ProgramError(3007, `${mint} is not a mint of ${tokenProgram}`);
    }
    return getMintDecoder().decode(account.data);
  };

  const createAssociatedIfMissing = (address: Address, mint: Address, owner: Address, tokenProgram: Address) => {
    if (!accounts.has(address)) {
      writeTokenAmount(address, mint, owner, tokenProgram, 0n);
    }
  };

  // The escrow program's three handlers, as their account constraints and
  // handler code define them.
  const runEscrowInstruction = async (instruction: Instruction, signers: Set<string>): Promise<Array<Address>> => {
    const withAccountsAndData = instruction as Parameters<typeof parseMakeOfferInstruction>[0];
    const kind = identifyEscrowInstruction(withAccountsAndData);

    if (kind === EscrowInstruction.MakeOffer) {
      const { accounts: named, data } = parseMakeOfferInstruction(withAccountsAndData);
      const maker = named.maker.address;
      const tokenProgram = named.tokenProgram.address;
      if (!signers.has(maker)) {
        throw new ProgramError(3010, "The maker did not sign");
      }
      if (named.makerTokenAccountA.address === named.makerTokenAccountB.address) {
        throw new ProgramError(2040, "ConstraintDuplicateMutableAccount");
      }
      requireMint(named.tokenMintA.address, tokenProgram);
      requireMint(named.tokenMintB.address, tokenProgram);
      const [offer] = await findOfferPda({ maker, id: data.id });
      if (named.offer.address !== offer) {
        throw new ProgramError(2006, "ConstraintSeeds: offer");
      }
      if (accounts.has(offer)) {
        throw new ProgramError(0, "The offer account already exists");
      }
      await requireAssociated(
        named.makerTokenAccountA.address,
        maker,
        named.tokenMintA.address,
        tokenProgram,
        "maker_token_account_a",
      );
      await requireAssociated(
        named.makerTokenAccountB.address,
        maker,
        named.tokenMintB.address,
        tokenProgram,
        "maker_token_account_b",
      );
      await requireAssociated(named.vault.address, offer, named.tokenMintA.address, tokenProgram, "vault");
      if (data.tokenAOfferedAmount === 0n || data.tokenBWantedAmount === 0n) {
        throw new ProgramError(6000, "ZeroAmount");
      }
      createAssociatedIfMissing(named.vault.address, named.tokenMintA.address, offer, tokenProgram);
      createAssociatedIfMissing(named.makerTokenAccountB.address, named.tokenMintB.address, maker, tokenProgram);
      transfer(named.makerTokenAccountA.address, named.vault.address, data.tokenAOfferedAmount);
      const [, bump] = await findOfferPda({ maker, id: data.id });
      const offerData = getOfferEncoder().encode({
        id: data.id,
        maker,
        tokenMintA: named.tokenMintA.address,
        tokenMintB: named.tokenMintB.address,
        tokenBWantedAmount: data.tokenBWantedAmount,
        bump,
      });
      accounts.set(offer, { owner: ESCROW_PROGRAM_ADDRESS, data: offerData, lamports: rentFor(offerData.length) });
      return [offer];
    }

    if (kind === EscrowInstruction.TakeOffer) {
      const { accounts: named, data } = parseTakeOfferInstruction(withAccountsAndData);
      const taker = named.taker.address;
      const tokenProgram = named.tokenProgram.address;
      if (!signers.has(taker)) {
        throw new ProgramError(3010, "The taker did not sign");
      }
      const offerAccount = accounts.get(named.offer.address);
      if (!offerAccount || offerAccount.owner !== ESCROW_PROGRAM_ADDRESS) {
        throw new ProgramError(3012, "AccountNotInitialized: offer");
      }
      const offer = getOfferDecoder().decode(offerAccount.data);
      if (named.maker.address !== offer.maker) {
        throw new ProgramError(2012, "ConstraintAddress: maker");
      }
      if (named.tokenMintA.address !== offer.tokenMintA || named.tokenMintB.address !== offer.tokenMintB) {
        throw new ProgramError(2012, "ConstraintAddress: mint");
      }
      const [expectedOffer] = await findOfferPda({ maker: offer.maker, id: offer.id });
      if (named.offer.address !== expectedOffer) {
        throw new ProgramError(2006, "ConstraintSeeds: offer");
      }
      await requireAssociated(
        named.takerTokenAccountA.address,
        taker,
        offer.tokenMintA,
        tokenProgram,
        "taker_token_account_a",
      );
      await requireAssociated(
        named.takerTokenAccountB.address,
        taker,
        offer.tokenMintB,
        tokenProgram,
        "taker_token_account_b",
      );
      await requireAssociated(
        named.makerTokenAccountB.address,
        offer.maker,
        offer.tokenMintB,
        tokenProgram,
        "maker_token_account_b",
      );
      await requireAssociated(named.vault.address, named.offer.address, offer.tokenMintA, tokenProgram, "vault");
      const vault = readToken(named.vault.address);
      if (!vault) {
        throw new ProgramError(3012, "AccountNotInitialized: vault");
      }
      if (vault.amount < data.minimumTokenAOut || offer.tokenBWantedAmount > data.maximumTokenBIn) {
        throw new ProgramError(6001, "OfferTermsChanged");
      }
      createAssociatedIfMissing(named.takerTokenAccountA.address, offer.tokenMintA, taker, tokenProgram);
      transfer(named.takerTokenAccountB.address, named.makerTokenAccountB.address, offer.tokenBWantedAmount);
      transfer(named.vault.address, named.takerTokenAccountA.address, vault.amount);
      accounts.delete(named.vault.address);
      accounts.delete(named.offer.address);
      return [named.offer.address];
    }

    if (kind === EscrowInstruction.CancelOffer) {
      const { accounts: named } = parseCancelOfferInstruction(withAccountsAndData);
      const maker = named.maker.address;
      const tokenProgram = named.tokenProgram.address;
      if (!signers.has(maker)) {
        throw new ProgramError(3010, "The maker did not sign");
      }
      const offerAccount = accounts.get(named.offer.address);
      if (!offerAccount || offerAccount.owner !== ESCROW_PROGRAM_ADDRESS) {
        throw new ProgramError(3012, "AccountNotInitialized: offer");
      }
      const offer = getOfferDecoder().decode(offerAccount.data);
      if (maker !== offer.maker) {
        throw new ProgramError(2012, "ConstraintAddress: maker");
      }
      await requireAssociated(
        named.makerTokenAccountA.address,
        maker,
        offer.tokenMintA,
        tokenProgram,
        "maker_token_account_a",
      );
      await requireAssociated(named.vault.address, named.offer.address, offer.tokenMintA, tokenProgram, "vault");
      const vault = readToken(named.vault.address);
      if (!vault) {
        throw new ProgramError(3012, "AccountNotInitialized: vault");
      }
      transfer(named.vault.address, named.makerTokenAccountA.address, vault.amount);
      accounts.delete(named.vault.address);
      accounts.delete(named.offer.address);
      return [named.offer.address];
    }

    throw new ProgramError(101, "InstructionFallbackNotFound");
  };

  // Run a transaction against a copy of the store, so a failure part-way
  // through leaves nothing changed, as on a real cluster.
  const execute = async (wireTransaction: string, commit: boolean) => {
    const transaction = getTransactionDecoder().decode(getBase64Encoder().encode(wireTransaction));
    const compiled = getCompiledTransactionMessageDecoder().decode(transaction.messageBytes);
    const message = decompileTransactionMessage(compiled);

    // A program sees an account as a signer when the message's header says it
    // is. Signatures are checked only when the transaction is sent: a
    // simulation runs unsigned, as Kit's planner simulates before signing.
    const signers = new Set<string>(Object.keys(transaction.signatures));
    if (commit) {
      for (const [signerAddress, signature] of Object.entries(transaction.signatures)) {
        const publicKey = await getPublicKeyFromAddress(signerAddress as Address);
        if (
          signature === null ||
          !(await verifySignature(publicKey, signature as SignatureBytes, transaction.messageBytes))
        ) {
          return {
            err: "SignatureFailure",
            logs: [`${signerAddress} did not sign`],
            signers,
            transaction,
          };
        }
      }
    }

    const snapshot = new Map(accounts);
    const logs: Array<string> = [];
    const changed: Array<Address> = [];
    try {
      for (const [index, instruction] of message.instructions.entries()) {
        if (instruction.programAddress === COMPUTE_BUDGET_PROGRAM) {
          continue;
        }
        if (instruction.programAddress !== ESCROW_PROGRAM_ADDRESS) {
          throw Object.assign(new ProgramError(0, `The mock cluster runs no ${instruction.programAddress}`), {
            index,
          });
        }
        logs.push(`Program ${ESCROW_PROGRAM_ADDRESS} invoke [1]`);
        try {
          changed.push(...(await runEscrowInstruction(instruction, signers)));
        } catch (error) {
          throw Object.assign(error as Error, { index });
        }
        logs.push(`Program ${ESCROW_PROGRAM_ADDRESS} success`);
      }
    } catch (error) {
      const failure = error as ProgramError & { index: number };
      accounts.clear();
      for (const [address, account] of snapshot) {
        accounts.set(address, account);
      }
      logs.push(`Program log: AnchorError: ${failure.message}. Error Number: ${failure.code}.`);
      logs.push(`Program ${ESCROW_PROGRAM_ADDRESS} failed: custom program error: 0x${failure.code.toString(16)}`);
      return { err: { InstructionError: [failure.index, { Custom: failure.code }] }, logs, signers, transaction };
    }
    if (!commit) {
      accounts.clear();
      for (const [address, account] of snapshot) {
        accounts.set(address, account);
      }
    } else {
      const signature = getBase58Decoder().decode(Object.values(transaction.signatures)[0] as ReadonlyUint8Array);
      transactions.push({ signature, instructions: [...message.instructions] });
      processedSignatures.add(signature);
      slot += 1n;
      notifyProgramSubscribers(changed);
    }
    return { err: null, logs, signers, transaction };
  };

  const simulationValue = (result: { err: unknown; logs: Array<string> }) => ({
    err: result.err,
    logs: result.logs,
    accounts: null,
    returnData: null,
    unitsConsumed: 40_000,
    loadedAccountsDataSize: 50_000,
  });

  const methods: Record<string, (params: Array<unknown>) => Promise<unknown> | unknown> = {
    getHealth: () => "ok",
    getSlot: () => Number(slot),
    getEpochInfo: () => ({
      absoluteSlot: Number(slot),
      blockHeight: Number(slot),
      epoch: 1,
      slotIndex: 0,
      slotsInEpoch: 432_000,
      transactionCount: transactions.length,
    }),
    getLatestBlockhash: () => ({
      context: context(),
      value: { blockhash: "EETubP5AKHgjPAhzPAFcb8BAY1hMH639CWCFTqi3hq1k", lastValidBlockHeight: Number(slot) + 150 },
    }),
    getMinimumBalanceForRentExemption: ([size]) => Number(rentFor(Number(size))),
    getBalance: ([address]) => ({
      context: context(),
      value: Number(accounts.get(address as Address)?.lamports ?? 0n),
    }),
    getAccountInfo: ([address]) => ({ context: context(), value: encodeAccount(accounts.get(address as Address)) }),
    getMultipleAccounts: ([addresses]) => ({
      context: context(),
      value: (addresses as Array<Address>).map((address) => encodeAccount(accounts.get(address))),
    }),
    getProgramAccounts: ([programAddress, config]) => {
      const { filters = [] } = (config ?? {}) as {
        filters?: Array<{ dataSize?: number; memcmp?: { offset: number; bytes: string } }>;
      };
      return [...accounts.entries()]
        .filter(([, account]) => account.owner === programAddress)
        .filter(([, account]) =>
          filters.every((filter) => {
            if (filter.dataSize !== undefined) {
              return account.data.length === Number(filter.dataSize);
            }
            if (filter.memcmp) {
              const expected = getBase58Encoder().encode(filter.memcmp.bytes);
              const offset = Number(filter.memcmp.offset);
              return expected.every((byte, index) => account.data[offset + index] === byte);
            }
            return true;
          }),
        )
        .map(([pubkey, account]) => ({ pubkey, account: encodeAccount(account) }));
    },
    getTokenAccountsByOwner: ([owner, filter]) => {
      const { programId } = filter as { programId: Address };
      const value = [...accounts.entries()]
        // A token account is 165 bytes, or longer with Token-2022's account
        // type byte, 2, at offset 165; a mint with extensions is longer too,
        // with a 1 there.
        .filter(
          ([, account]) =>
            account.owner === programId &&
            (account.data.length === 165 || (account.data.length > 165 && account.data[165] === 2)),
        )
        .flatMap(([pubkey, account]) => {
          const token = getTokenDecoder().decode(account.data);
          if (token.owner !== owner) {
            return [];
          }
          const mint = getMintDecoder().decode((accounts.get(token.mint) as StoredAccount).data);
          return [
            {
              pubkey,
              account: {
                owner: account.owner,
                lamports: Number(account.lamports),
                executable: false,
                rentEpoch: 0,
                space: account.data.length,
                data: {
                  program: "spl-token",
                  space: account.data.length,
                  parsed: {
                    type: "account",
                    info: {
                      mint: token.mint,
                      owner: token.owner,
                      state: "initialized",
                      isNative: false,
                      tokenAmount: {
                        amount: token.amount.toString(),
                        decimals: mint.decimals,
                        uiAmount: Number(token.amount) / 10 ** mint.decimals,
                        uiAmountString: String(Number(token.amount) / 10 ** mint.decimals),
                      },
                    },
                  },
                },
              },
            },
          ];
        });
      return { context: context(), value };
    },
    simulateTransaction: async ([wireTransaction]) => ({
      context: context(),
      value: simulationValue(await execute(wireTransaction as string, false)),
    }),
    sendTransaction: async ([wireTransaction]) => {
      const result = await execute(wireTransaction as string, true);
      if (result.err) {
        throw Object.assign(new Error("Transaction simulation failed"), {
          rpcError: { code: -32002, message: "Transaction simulation failed", data: simulationValue(result) },
        });
      }
      return getBase58Decoder().decode(Object.values(result.transaction.signatures)[0] as ReadonlyUint8Array);
    },
    getSignatureStatuses: ([signatures]) => ({
      context: context(),
      value: (signatures as Array<string>).map((signature) =>
        processedSignatures.has(signature)
          ? { slot: Number(slot), confirmations: null, err: null, confirmationStatus: "finalized" }
          : null,
      ),
    }),
    requestAirdrop: ([address, amount]) => {
      const existing = accounts.get(address as Address);
      accounts.set(address as Address, {
        owner: (existing?.owner ?? SYSTEM_PROGRAM) as Address,
        data: existing?.data ?? new Uint8Array(),
        lamports: (existing?.lamports ?? 0n) + BigInt(amount as number),
      });
      const signature = getBase58Decoder().decode(crypto.getRandomValues(new Uint8Array(64)));
      processedSignatures.add(signature);
      return signature;
    },
  };

  const readBody = (request: IncomingMessage) =>
    new Promise<string>((resolve) => {
      let body = "";
      request.on("data", (chunk) => {
        body += chunk;
      });
      request.on("end", () => resolve(body));
    });

  const server: Server = createServer(async (request, response) => {
    response.setHeader("Access-Control-Allow-Origin", "*");
    response.setHeader("Access-Control-Allow-Headers", "*");
    if (request.method === "OPTIONS") {
      response.writeHead(204).end();
      return;
    }
    const call = JSON.parse(await readBody(request)) as { id: number; method: string; params: Array<unknown> };
    const handler = methods[call.method];
    response.setHeader("Content-Type", "application/json");
    if (!handler) {
      response.end(
        JSON.stringify({ jsonrpc: "2.0", id: call.id, error: { code: -32601, message: "Method not found" } }),
      );
      return;
    }
    try {
      const result = await handler(call.params ?? []);
      response.end(JSON.stringify({ jsonrpc: "2.0", id: call.id, result }));
    } catch (error) {
      const rpcError = (error as { rpcError?: unknown }).rpcError ?? { code: -32603, message: String(error) };
      response.end(JSON.stringify({ jsonrpc: "2.0", id: call.id, error: rpcError }));
    }
  });

  const subscriptionServer = new WebSocketServer({ server });
  subscriptionServer.on("connection", (socket) => {
    socket.on("message", (raw) => {
      let call: { id: number; method: string; params: Array<unknown> };
      try {
        call = JSON.parse(raw.toString());
      } catch {
        return;
      }
      if (call.method.endsWith("Unsubscribe")) {
        subscriptions.delete(Number(call.params[0]));
        socket.send(JSON.stringify({ jsonrpc: "2.0", id: call.id, result: true }));
        return;
      }
      const id = nextSubscriptionId++;
      subscriptions.set(id, { socket, method: call.method, params: call.params });
      socket.send(JSON.stringify({ jsonrpc: "2.0", id: call.id, result: id }));
      if (call.method === "signatureSubscribe" && processedSignatures.has(call.params[0] as string)) {
        setTimeout(() => {
          socket.send(
            JSON.stringify({
              jsonrpc: "2.0",
              method: "signatureNotification",
              params: { subscription: id, result: { context: context(), value: { err: null } } },
            }),
          );
        }, 20);
      }
    });
    socket.on("close", () => {
      for (const [id, subscription] of subscriptions) {
        if (subscription.socket === socket) {
          subscriptions.delete(id);
        }
      }
    });
  });

  await new Promise<void>((resolve) => server.listen(port, "127.0.0.1", resolve));
  const { port: boundPort } = server.address() as AddressInfo;

  return {
    rpcUrl: `http://127.0.0.1:${boundPort}`,
    rpcSubscriptionsUrl: `ws://127.0.0.1:${boundPort}`,
    accounts,
    transactions,
    close: async () => {
      for (const client of subscriptionServer.clients) {
        client.terminate();
      }
      await new Promise<void>((resolve) => subscriptionServer.close(() => resolve()));
      await new Promise<void>((resolve) => server.close(() => resolve()));
    },
  };
};
