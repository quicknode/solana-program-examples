import { PublicKey } from '@solana/web3.js'
import {
  getAssociatedTokenAddressSync,
  TOKEN_PROGRAM_ID,
  ASSOCIATED_TOKEN_PROGRAM_ID,
} from '@solana/spl-token'
import { VAULT_PROGRAM_ID, ROUTER_PROGRAM_ID } from './config'

const seed = (s: string): Buffer => Buffer.from(s, 'utf8')

function u64le(n: bigint | number): Buffer {
  const buf = Buffer.alloc(8)
  buf.writeBigUInt64LE(BigInt(n))
  return buf
}

const pda = (seeds: Array<Buffer | Uint8Array>, programId: PublicKey): PublicKey =>
  PublicKey.findProgramAddressSync(seeds, programId)[0]

// ---- vault-strategy program PDAs -------------------------------------------

/** ["strategy", index_u64_le] */
export const strategyPda = (index: bigint): PublicKey =>
  pda([seed('strategy'), u64le(index)], VAULT_PROGRAM_ID)

/** ["share_mint", strategy] */
export const shareMintPda = (strategy: PublicKey): PublicKey =>
  pda([seed('share_mint'), strategy.toBuffer()], VAULT_PROGRAM_ID)

/** ["asset", strategy, index_u8] */
export const assetConfigPda = (strategy: PublicKey, index: number): PublicKey =>
  pda([seed('asset'), strategy.toBuffer(), Buffer.from([index])], VAULT_PROGRAM_ID)

/** ["registry", authority] */
export const registryPda = (authority: PublicKey): PublicKey =>
  pda([seed('registry'), authority.toBuffer()], VAULT_PROGRAM_ID)

/** ["approved_asset", registry, mint] — existence == approved */
export const approvedAssetPda = (registry: PublicKey, mint: PublicKey): PublicKey =>
  pda([seed('approved_asset'), registry.toBuffer(), mint.toBuffer()], VAULT_PROGRAM_ID)

// ---- mock-swap-router program PDAs -----------------------------------------
// The strategy stores which router it uses, so these accept the router program id
// (defaulting to the configured one) to stay correct if a strategy points elsewhere.

/** ["router_config"] */
export const routerConfigPda = (routerProgram: PublicKey = ROUTER_PROGRAM_ID): PublicKey =>
  pda([seed('router_config')], routerProgram)

/** ["router_authority"] */
export const routerAuthorityPda = (routerProgram: PublicKey = ROUTER_PROGRAM_ID): PublicKey =>
  pda([seed('router_authority')], routerProgram)

/** ["rate", mint] */
export const assetRatePda = (mint: PublicKey, routerProgram: PublicKey = ROUTER_PROGRAM_ID): PublicKey =>
  pda([seed('rate'), mint.toBuffer()], routerProgram)

// ---- associated token accounts ---------------------------------------------

/** Strategy-owned vault ATA for a mint (strategy is a PDA → allowOwnerOffCurve). */
export const vaultAta = (mint: PublicKey, strategy: PublicKey): PublicKey =>
  getAssociatedTokenAddressSync(mint, strategy, true)

/** A wallet's ATA for a mint. */
export const userAta = (mint: PublicKey, owner: PublicKey): PublicKey =>
  getAssociatedTokenAddressSync(mint, owner, false)

/** Router USDC treasury = ATA(usdc, router_authority). */
export const routerUsdcTreasury = (
  usdcMint: PublicKey,
  routerProgram: PublicKey = ROUTER_PROGRAM_ID,
): PublicKey => getAssociatedTokenAddressSync(usdcMint, routerAuthorityPda(routerProgram), true)

export { TOKEN_PROGRAM_ID, ASSOCIATED_TOKEN_PROGRAM_ID }
