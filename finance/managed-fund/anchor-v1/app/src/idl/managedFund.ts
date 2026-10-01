import type { Idl } from "@coral-xyz/anchor";
import idlJson from "./managed_fund.json";

// The IDL, generated from the program source by `anchor idl build` (see the app
// README for the regeneration command). `address` is overridden
// at runtime by the configured program id (see src/solana/program.ts), so a fresh
// devnet deploy under a new id works without editing this file.
export const MANAGED_FUND_IDL = idlJson as Idl;

import type { BN } from "@coral-xyz/anchor";
// Typed shapes of the on-chain accounts, matching the `types` section of the IDL.
// (Anchor decodes to these; we cast fetch results to them for ergonomics.)
import type { PublicKey } from "@solana/web3.js";

export interface FundAccount {
  index: BN;
  manager: PublicKey;
  registry: PublicKey;
  shareMint: PublicKey;
  usdcMint: PublicKey;
  /** The USDC mint's decimals; valuation scales every asset into these minor units. */
  usdcDecimals: number;
  swapRouter: PublicKey;
  feeBps: number;
  maxSlippageBps: number;
  /** How far (bps of fund value) an asset must sit above target before rebalance may sell it. */
  rebalanceThresholdBps: number;
  totalShares: BN;
  /** USDC the program has recorded in the USDC vault; excludes donations. */
  usdcHoldings: BN;
  /** Each asset's recorded holding, indexed by asset index; excludes donations. */
  assetHoldings: BN[];
  lastFeeAccrualTimestamp: BN;
  assetCount: number;
  totalWeightBps: number;
  bump: number;
}

export interface AssetConfigAccount {
  fund: PublicKey;
  index: number;
  mint: PublicKey;
  /** The asset mint's decimals. */
  decimals: number;
  priceFeed: PublicKey;
  vault: PublicKey;
  weightBps: number;
  bump: number;
}

export interface RegistryAccount {
  authority: PublicKey;
  bump: number;
}

export interface ApprovedAssetAccount {
  registry: PublicKey;
  mint: PublicKey;
  priceFeed: PublicKey;
  bump: number;
}
