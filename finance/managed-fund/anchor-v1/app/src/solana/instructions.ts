import { BN } from "@coral-xyz/anchor";
import { createAssociatedTokenAccountIdempotentInstruction } from "@solana/spl-token";
import { type AccountMeta, type PublicKey, SystemProgram, type TransactionInstruction } from "@solana/web3.js";
import type { FundView } from "./fund";
import {
  ASSOCIATED_TOKEN_PROGRAM_ID,
  approvedAssetPda,
  assetConfigPda,
  assetRatePda,
  fundPda,
  registryPda,
  routerConfigPda,
  routerUsdcTreasury,
  shareMintPda,
  TOKEN_PROGRAM_ID,
  userAta,
  vaultAta,
} from "./pdas";
import type { FundProgram } from "./program";

const bn = (v: bigint): BN => new BN(v.toString());
const ro = (pubkey: PublicKey): AccountMeta => ({ pubkey, isSigner: false, isWritable: false });
const rw = (pubkey: PublicKey): AccountMeta => ({ pubkey, isSigner: false, isWritable: true });

// The three account keys every token-touching instruction shares.
const TOKEN_PROGRAMS = {
  associatedTokenProgram: ASSOCIATED_TOKEN_PROGRAM_ID,
  tokenProgram: TOKEN_PROGRAM_ID,
  systemProgram: SystemProgram.programId,
};

function requireAccount(view: FundView) {
  if (!view.account) throw new Error("fund does not exist on this cluster");
  return view.account;
}

// ---- depositor -------------------------------------------------------------

/**
 * deposit(usdc_amount, minimum_shares). remaining_accounts per asset, in the order the
 * program reads: [asset_config(ro), vault(rw), mint(rw), rate(ro), price_feed(ro)].
 */
export function buildDepositIx(
  program: FundProgram,
  view: FundView,
  depositor: PublicKey,
  usdcAmount: bigint,
  minimumShares: bigint,
): Promise<TransactionInstruction> {
  const s = requireAccount(view);
  const router = s.swapRouter;
  const remaining: AccountMeta[] = [];
  for (const a of view.assets) {
    remaining.push(ro(a.config), rw(a.vault), rw(a.mint), ro(assetRatePda(a.mint, router)), ro(a.priceFeed));
  }
  return program.methods
    .deposit(bn(usdcAmount), bn(minimumShares))
    .accountsStrict({
      depositor,
      fund: view.fund,
      shareMint: view.shareMint,
      usdcMint: s.usdcMint,
      depositorUsdcAccount: userAta(s.usdcMint, depositor),
      depositorShareAccount: userAta(view.shareMint, depositor),
      vaultUsdc: view.usdcVault,
      routerConfig: routerConfigPda(router),
      routerUsdcTreasury: routerUsdcTreasury(s.usdcMint, router),
      swapRouterProgram: router,
      ...TOKEN_PROGRAMS,
    })
    .remainingAccounts(remaining)
    .instruction();
}

/**
 * withdraw(shares_to_burn, min_usdc_out). The program pays in kind, so the user must
 * already hold a token account for every asset — we create them idempotently first.
 * remaining_accounts per asset: [asset_config(ro), vault(rw), mint(ro), user_ata(rw)].
 * Returns the ATA-creation instructions followed by the withdraw instruction.
 */
export async function buildWithdrawIxs(
  program: FundProgram,
  view: FundView,
  user: PublicKey,
  sharesToBurn: bigint,
  minUsdcOut: bigint,
): Promise<TransactionInstruction[]> {
  const s = requireAccount(view);
  const pre: TransactionInstruction[] = [];
  const usdcAta = userAta(s.usdcMint, user);
  pre.push(createAssociatedTokenAccountIdempotentInstruction(user, usdcAta, user, s.usdcMint));

  const remaining: AccountMeta[] = [];
  for (const a of view.assets) {
    const ata = userAta(a.mint, user);
    pre.push(createAssociatedTokenAccountIdempotentInstruction(user, ata, user, a.mint));
    remaining.push(ro(a.config), rw(a.vault), ro(a.mint), rw(ata));
  }

  const ix = await program.methods
    .withdraw(bn(sharesToBurn), bn(minUsdcOut))
    .accountsStrict({
      user,
      fund: view.fund,
      shareMint: view.shareMint,
      usdcMint: s.usdcMint,
      userShareAccount: userAta(view.shareMint, user),
      userUsdcAccount: usdcAta,
      vaultUsdc: view.usdcVault,
      ...TOKEN_PROGRAMS,
    })
    .remainingAccounts(remaining)
    .instruction();

  return [...pre, ix];
}

// ---- manager ---------------------------------------------------------------

/**
 * rebalance(sell_index, buy_index): permissionless. The program values every asset and
 * sizes the trade itself, so it takes the same remaining_accounts as deposit, per asset:
 * [asset_config(ro), vault(rw), mint(rw), rate(ro), price_feed(ro)].
 */
export function buildRebalanceIx(
  program: FundProgram,
  view: FundView,
  caller: PublicKey,
  sellIndex: number,
  buyIndex: number,
): Promise<TransactionInstruction> {
  const s = requireAccount(view);
  const router = s.swapRouter;
  if (!view.assets[sellIndex] || !view.assets[buyIndex]) throw new Error("sell/buy asset index out of range");
  const remaining: AccountMeta[] = [];
  for (const a of view.assets) {
    remaining.push(ro(a.config), rw(a.vault), rw(a.mint), ro(assetRatePda(a.mint, router)), ro(a.priceFeed));
  }
  return program.methods
    .rebalance(sellIndex, buyIndex)
    .accountsStrict({
      caller,
      fund: view.fund,
      usdcMint: s.usdcMint,
      vaultUsdc: view.usdcVault,
      routerConfig: routerConfigPda(router),
      routerUsdcTreasury: routerUsdcTreasury(s.usdcMint, router),
      swapRouterProgram: router,
      ...TOKEN_PROGRAMS,
    })
    .remainingAccounts(remaining)
    .instruction();
}

/** set_weight(weight_bps): reweight an asset, or set 0 to retire it. */
export function buildSetWeightIx(
  program: FundProgram,
  view: FundView,
  manager: PublicKey,
  assetIndex: number,
  weightBps: number,
): Promise<TransactionInstruction> {
  return program.methods
    .setWeight(weightBps)
    .accountsStrict({
      manager,
      fund: view.fund,
      assetConfig: assetConfigPda(view.fund, assetIndex),
    })
    .instruction();
}

/** add_asset(weight_bps): register a curator-approved mint at the next index. */
export function buildAddAssetIx(
  program: FundProgram,
  view: FundView,
  manager: PublicKey,
  assetMint: PublicKey,
  weightBps: number,
): Promise<TransactionInstruction> {
  const s = requireAccount(view);
  const registry = s.registry;
  return program.methods
    .addAsset(weightBps)
    .accountsStrict({
      manager,
      fund: view.fund,
      registry,
      assetMint,
      approvedAsset: approvedAssetPda(registry, assetMint),
      assetConfig: assetConfigPda(view.fund, s.assetCount),
      vaultAsset: vaultAta(assetMint, view.fund),
      ...TOKEN_PROGRAMS,
    })
    .instruction();
}

/** collect_fees(): permissionless — anyone pays to mint the accrued fee to the manager. */
export function buildCollectFeesIx(
  program: FundProgram,
  view: FundView,
  payer: PublicKey,
): Promise<TransactionInstruction> {
  const s = requireAccount(view);
  return program.methods
    .collectFees()
    .accountsStrict({
      manager: s.manager,
      fund: view.fund,
      shareMint: view.shareMint,
      managerShareAccount: userAta(view.shareMint, s.manager),
      payer,
      ...TOKEN_PROGRAMS,
    })
    .instruction();
}

export interface InitializeFundParams {
  manager: PublicKey;
  usdcMint: PublicKey;
  registry: PublicKey;
  index: bigint;
  feeBps: number;
  maxSlippageBps: number;
  rebalanceThresholdBps: number;
  swapRouter: PublicKey;
}

/** initialize_fund(index, fee_bps, max_slippage_bps, rebalance_threshold_bps, swap_router). */
export function buildInitializeFundIx(program: FundProgram, p: InitializeFundParams): Promise<TransactionInstruction> {
  const fund = fundPda(p.index);
  return program.methods
    .initializeFund(bn(p.index), p.feeBps, p.maxSlippageBps, p.rebalanceThresholdBps, p.swapRouter)
    .accountsStrict({
      manager: p.manager,
      usdcMint: p.usdcMint,
      registry: p.registry,
      fund,
      shareMint: shareMintPda(fund),
      vaultUsdc: vaultAta(p.usdcMint, fund),
      ...TOKEN_PROGRAMS,
    })
    .instruction();
}

// ---- curator (registry) — used by seeding / a future curator surface --------

/** initialize_registry(): create the curator record owned by `authority`. */
export function buildInitializeRegistryIx(program: FundProgram, authority: PublicKey): Promise<TransactionInstruction> {
  return program.methods
    .initializeRegistry()
    .accountsStrict({
      authority,
      registry: registryPda(authority),
      systemProgram: SystemProgram.programId,
    })
    .instruction();
}

/** approve_asset(price_feed): bind a mint to its official Pyth feed. */
export function buildApproveAssetIx(
  program: FundProgram,
  authority: PublicKey,
  assetMint: PublicKey,
  priceFeed: PublicKey,
): Promise<TransactionInstruction> {
  const registry = registryPda(authority);
  return program.methods
    .approveAsset(priceFeed)
    .accountsStrict({
      authority,
      registry,
      assetMint,
      approvedAsset: approvedAssetPda(registry, assetMint),
      systemProgram: SystemProgram.programId,
    })
    .instruction();
}
