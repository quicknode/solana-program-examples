import type { Commitment } from "@solana/web3.js";
import { PublicKey } from "@solana/web3.js";

// Repo localnet ids (Anchor.toml / declare_id!). Overridden by env after a devnet deploy.
const DEFAULT_VAULT_PROGRAM = "VLT5W7bqhRN4nCdRpXm8UfHRxZd9EuZGqiSAkGHQfGh";
const DEFAULT_ROUTER_PROGRAM = "SWPR8Rk3aq3DrDGLdaANq7xCMnXoUFUJWJJmCWxc8Jm";

const env = import.meta.env;

function optionalPubkey(value: string | undefined): PublicKey | null {
  if (!value) return null;
  return new PublicKey(value); // throws on a malformed value — fail loud, not silent
}

/** RPC endpoint. Defaults to public devnet; point VITE_RPC_URL at your Quicknode devnet. */
export const RPC_URL: string = env.VITE_RPC_URL || "https://api.devnet.solana.com";

export const VAULT_PROGRAM_ID = new PublicKey(env.VITE_VAULT_PROGRAM_ID || DEFAULT_VAULT_PROGRAM);
export const ROUTER_PROGRAM_ID = new PublicKey(env.VITE_ROUTER_PROGRAM_ID || DEFAULT_ROUTER_PROGRAM);

/** The USDC mint the strategy was created with. Null until configured (post-seed). */
export const USDC_MINT: PublicKey | null = optionalPubkey(env.VITE_USDC_MINT);

/** PDA seed for the strategy under view ("strategy" + index). */
export const STRATEGY_INDEX: bigint = BigInt(env.VITE_STRATEGY_INDEX ?? "0");

export const CLUSTER: string = env.VITE_CLUSTER || "devnet";
export const COMMITMENT: Commitment = "confirmed";

// Program constants, mirrored from the Rust source (state/*.rs, instructions/*.rs).
export const MAX_ASSETS = 16;
export const MAX_FEE_BPS = 1_000; // 10%
export const MAX_SLIPPAGE_BPS = 1_000; // 10%
export const BPS_DENOMINATOR = 10_000;
export const PYTH_PRICE_PRECISION = 100_000_000n; // 10^8, Pyth exponent -8
export const MAX_PRICE_AGE_SECONDS = 60;
export const SHARE_DECIMALS = 6; // share_mint is created with mint::decimals = 6
