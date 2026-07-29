import { BN } from "@coral-xyz/anchor";
import { createAssociatedTokenAccountIdempotentInstruction } from "@solana/spl-token";
import { type AccountMeta, type PublicKey, SystemProgram, type TransactionInstruction } from "@solana/web3.js";
import {
  ASSOCIATED_TOKEN_PROGRAM_ID,
  approvedAssetPda,
  assetConfigPda,
  assetRatePda,
  registryPda,
  routerAuthorityPda,
  routerConfigPda,
  routerUsdcTreasury,
  shareMintPda,
  strategyPda,
  TOKEN_PROGRAM_ID,
  userAta,
  vaultAta,
} from "./pdas";
import type { VaultProgram } from "./program";
import type { StrategyView } from "./strategy";

const bn = (v: bigint): BN => new BN(v.toString());
const ro = (pubkey: PublicKey): AccountMeta => ({ pubkey, isSigner: false, isWritable: false });
const rw = (pubkey: PublicKey): AccountMeta => ({ pubkey, isSigner: false, isWritable: true });

// The three account keys every token-touching instruction shares.
const TOKEN_PROGRAMS = {
  associatedTokenProgram: ASSOCIATED_TOKEN_PROGRAM_ID,
  tokenProgram: TOKEN_PROGRAM_ID,
  systemProgram: SystemProgram.programId,
};

function requireAccount(view: StrategyView) {
  if (!view.account) throw new Error("strategy does not exist on this cluster");
  return view.account;
}

// ---- depositor -------------------------------------------------------------

/**
 * deposit(usdc_amount, minimum_shares). remaining_accounts per asset, in the order the
 * program reads: [asset_config(ro), vault(rw), mint(rw), rate(ro), price_feed(ro)].
 */
export function buildDepositIx(
  program: VaultProgram,
  view: StrategyView,
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
      strategy: view.strategy,
      shareMint: view.shareMint,
      usdcMint: s.usdcMint,
      depositorUsdcAccount: userAta(s.usdcMint, depositor),
      depositorShareAccount: userAta(view.shareMint, depositor),
      vaultUsdc: view.usdcVault,
      routerConfig: routerConfigPda(router),
      routerUsdcTreasury: routerUsdcTreasury(s.usdcMint, router),
      routerAuthority: routerAuthorityPda(router),
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
  program: VaultProgram,
  view: StrategyView,
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
      strategy: view.strategy,
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

/** rebalance(sell_amount, usdc_to_invest): sell one asset for USDC, buy another. */
export function buildRebalanceIx(
  program: VaultProgram,
  view: StrategyView,
  manager: PublicKey,
  sellIndex: number,
  buyIndex: number,
  sellAmount: bigint,
  usdcToInvest: bigint,
): Promise<TransactionInstruction> {
  const s = requireAccount(view);
  const router = s.swapRouter;
  const sell = view.assets[sellIndex];
  const buy = view.assets[buyIndex];
  if (!sell || !buy) throw new Error("sell/buy asset index out of range");
  return program.methods
    .rebalance(bn(sellAmount), bn(usdcToInvest))
    .accountsStrict({
      manager,
      strategy: view.strategy,
      usdcMint: s.usdcMint,
      sellMint: sell.mint,
      buyMint: buy.mint,
      sellConfig: sell.config,
      buyConfig: buy.config,
      sellPriceFeed: sell.priceFeed,
      buyPriceFeed: buy.priceFeed,
      vaultSell: sell.vault,
      vaultBuy: buy.vault,
      vaultUsdc: view.usdcVault,
      sellRate: assetRatePda(sell.mint, router),
      buyRate: assetRatePda(buy.mint, router),
      routerConfig: routerConfigPda(router),
      routerUsdcTreasury: routerUsdcTreasury(s.usdcMint, router),
      routerAuthority: routerAuthorityPda(router),
      swapRouterProgram: router,
      ...TOKEN_PROGRAMS,
    })
    .instruction();
}

/** set_weight(weight_bps): reweight an asset, or set 0 to retire it. */
export function buildSetWeightIx(
  program: VaultProgram,
  view: StrategyView,
  manager: PublicKey,
  assetIndex: number,
  weightBps: number,
): Promise<TransactionInstruction> {
  return program.methods
    .setWeight(weightBps)
    .accountsStrict({
      manager,
      strategy: view.strategy,
      assetConfig: assetConfigPda(view.strategy, assetIndex),
    })
    .instruction();
}

/** add_asset(weight_bps): register a curator-approved mint at the next index. */
export function buildAddAssetIx(
  program: VaultProgram,
  view: StrategyView,
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
      strategy: view.strategy,
      registry,
      assetMint,
      approvedAsset: approvedAssetPda(registry, assetMint),
      assetConfig: assetConfigPda(view.strategy, s.assetCount),
      vaultAsset: vaultAta(assetMint, view.strategy),
      ...TOKEN_PROGRAMS,
    })
    .instruction();
}

/** collect_fees(): permissionless — anyone pays to mint the accrued fee to the manager. */
export function buildCollectFeesIx(
  program: VaultProgram,
  view: StrategyView,
  payer: PublicKey,
): Promise<TransactionInstruction> {
  const s = requireAccount(view);
  return program.methods
    .collectFees()
    .accountsStrict({
      manager: s.manager,
      strategy: view.strategy,
      shareMint: view.shareMint,
      managerShareAccount: userAta(view.shareMint, s.manager),
      payer,
      ...TOKEN_PROGRAMS,
    })
    .instruction();
}

export interface InitializeStrategyParams {
  manager: PublicKey;
  usdcMint: PublicKey;
  registry: PublicKey;
  index: bigint;
  feeBps: number;
  maxSlippageBps: number;
  swapRouter: PublicKey;
}

/** initialize_strategy(index, fee_bps, max_slippage_bps, swap_router). */
export function buildInitializeStrategyIx(
  program: VaultProgram,
  p: InitializeStrategyParams,
): Promise<TransactionInstruction> {
  const strategy = strategyPda(p.index);
  return program.methods
    .initializeStrategy(bn(p.index), p.feeBps, p.maxSlippageBps, p.swapRouter)
    .accountsStrict({
      manager: p.manager,
      usdcMint: p.usdcMint,
      registry: p.registry,
      strategy,
      shareMint: shareMintPda(strategy),
      vaultUsdc: vaultAta(p.usdcMint, strategy),
      ...TOKEN_PROGRAMS,
    })
    .instruction();
}

// ---- curator (registry) — used by seeding / a future curator surface --------

/** initialize_registry(): create the curator record owned by `authority`. */
export function buildInitializeRegistryIx(
  program: VaultProgram,
  authority: PublicKey,
): Promise<TransactionInstruction> {
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
  program: VaultProgram,
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
