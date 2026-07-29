import type { Idl } from "@coral-xyz/anchor";
import { AnchorProvider, Program } from "@coral-xyz/anchor";
import { Connection, Keypair, type PublicKey, type Transaction, type VersionedTransaction } from "@solana/web3.js";
import { VAULT_STRATEGY_IDL } from "../idl/vaultStrategy";
import { COMMITMENT, RPC_URL, VAULT_PROGRAM_ID } from "./config";

export interface AnchorWalletLike {
  publicKey: PublicKey;
  signTransaction: <T extends Transaction | VersionedTransaction>(tx: T) => Promise<T>;
  signAllTransactions: <T extends Transaction | VersionedTransaction>(txs: T[]) => Promise<T[]>;
}

// Use the configured program id, not the value baked into the JSON, so a devnet
// redeploy under a new id needs only an env change.
function idl(): Idl {
  return { ...(VAULT_STRATEGY_IDL as Idl), address: VAULT_PROGRAM_ID.toBase58() };
}

/** A wallet stand-in for read-only use before a real wallet connects. */
function readonlyWallet(): AnchorWalletLike {
  const kp = Keypair.generate();
  return {
    publicKey: kp.publicKey,
    signTransaction: async (tx) => tx,
    signAllTransactions: async (txs) => txs,
  };
}

export function makeConnection(): Connection {
  return new Connection(RPC_URL, COMMITMENT);
}

export function makeProvider(connection: Connection, wallet?: AnchorWalletLike): AnchorProvider {
  return new AnchorProvider(connection, wallet ?? readonlyWallet(), {
    commitment: COMMITMENT,
    preflightCommitment: COMMITMENT,
  });
}

/**
 * Anchor Program with loosely-typed `methods`/`account` namespaces. The strong
 * per-instruction types normally come from `anchor build` codegen, which is
 * unavailable in this environment; typed reads live in strategy.ts and typed
 * builders in instructions.ts instead.
 */
export type VaultProgram = Omit<Program, "methods" | "account"> & {
  methods: any;
  account: any;
};

/** Build the Program. Pass a wallet to send transactions; omit it for reads. */
export function getProgram(connection: Connection, wallet?: AnchorWalletLike): VaultProgram {
  return new Program(idl(), makeProvider(connection, wallet)) as unknown as VaultProgram;
}
