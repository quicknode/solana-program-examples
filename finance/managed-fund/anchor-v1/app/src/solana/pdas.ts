import { ASSOCIATED_TOKEN_PROGRAM_ID, getAssociatedTokenAddressSync, TOKEN_PROGRAM_ID } from "@solana/spl-token";
import { PublicKey } from "@solana/web3.js";
import { FUND_PROGRAM_ID, ROUTER_PROGRAM_ID } from "./config";

const seed = (s: string): Buffer => Buffer.from(s, "utf8");

function u64le(n: bigint | number): Buffer {
  const buf = Buffer.alloc(8);
  buf.writeBigUInt64LE(BigInt(n));
  return buf;
}

const pda = (seeds: Array<Buffer | Uint8Array>, programId: PublicKey): PublicKey =>
  PublicKey.findProgramAddressSync(seeds, programId)[0];

// ---- managed-fund program PDAs -------------------------------------------

/** ["fund", index_u64_le] */
export const fundPda = (index: bigint): PublicKey => pda([seed("fund"), u64le(index)], FUND_PROGRAM_ID);

/** ["share_mint", fund] */
export const shareMintPda = (fund: PublicKey): PublicKey => pda([seed("share_mint"), fund.toBuffer()], FUND_PROGRAM_ID);

/** ["asset", fund, index_u8] */
export const assetConfigPda = (fund: PublicKey, index: number): PublicKey =>
  pda([seed("asset"), fund.toBuffer(), Buffer.from([index])], FUND_PROGRAM_ID);

/** ["registry", authority] */
export const registryPda = (authority: PublicKey): PublicKey =>
  pda([seed("registry"), authority.toBuffer()], FUND_PROGRAM_ID);

/** ["approved_asset", registry, mint] — existence == approved */
export const approvedAssetPda = (registry: PublicKey, mint: PublicKey): PublicKey =>
  pda([seed("approved_asset"), registry.toBuffer(), mint.toBuffer()], FUND_PROGRAM_ID);

// ---- mock-swap-router program PDAs -----------------------------------------
// The fund stores which router it uses, so these accept the router program id
// (defaulting to the configured one) to stay correct if a fund points elsewhere.

/** ["router_config"] — also owns the router's treasury and signs its token CPIs */
export const routerConfigPda = (routerProgram: PublicKey = ROUTER_PROGRAM_ID): PublicKey =>
  pda([seed("router_config")], routerProgram);

/** ["rate", mint] */
export const assetRatePda = (mint: PublicKey, routerProgram: PublicKey = ROUTER_PROGRAM_ID): PublicKey =>
  pda([seed("rate"), mint.toBuffer()], routerProgram);

// ---- associated token accounts ---------------------------------------------

/** Fund-owned vault ATA for a mint (fund is a PDA → allowOwnerOffCurve). */
export const vaultAta = (mint: PublicKey, fund: PublicKey): PublicKey =>
  getAssociatedTokenAddressSync(mint, fund, true);

/** A wallet's ATA for a mint. */
export const userAta = (mint: PublicKey, owner: PublicKey): PublicKey =>
  getAssociatedTokenAddressSync(mint, owner, false);

/** Router USDC treasury = ATA(usdc, router_config). */
export const routerUsdcTreasury = (usdcMint: PublicKey, routerProgram: PublicKey = ROUTER_PROGRAM_ID): PublicKey =>
  getAssociatedTokenAddressSync(usdcMint, routerConfigPda(routerProgram), true);

export { ASSOCIATED_TOKEN_PROGRAM_ID, TOKEN_PROGRAM_ID };
