import type { Idl } from '@coral-xyz/anchor'
import idlJson from './vault_strategy.json'

// The hand-authored IDL, transcribed from the program source. `address` is overridden
// at runtime by the configured program id (see src/solana/program.ts), so a fresh
// devnet deploy under a new id works without editing this file.
export const VAULT_STRATEGY_IDL = idlJson as Idl

// Typed shapes of the on-chain accounts, matching the `types` section of the IDL.
// (Anchor decodes to these; we cast fetch results to them for ergonomics.)
import type { PublicKey } from '@solana/web3.js'
import type { BN } from '@coral-xyz/anchor'

export interface StrategyAccount {
  index: BN
  manager: PublicKey
  registry: PublicKey
  shareMint: PublicKey
  usdcMint: PublicKey
  swapRouter: PublicKey
  feeBps: number
  maxSlippageBps: number
  totalShares: BN
  lastFeeAccrualTimestamp: BN
  assetCount: number
  totalWeightBps: number
  bump: number
}

export interface AssetConfigAccount {
  strategy: PublicKey
  index: number
  mint: PublicKey
  priceFeed: PublicKey
  vault: PublicKey
  weightBps: number
  bump: number
}

export interface RegistryAccount {
  authority: PublicKey
  bump: number
}

export interface ApprovedAssetAccount {
  registry: PublicKey
  mint: PublicKey
  priceFeed: PublicKey
  bump: number
}
