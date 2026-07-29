import type { BN } from "@coral-xyz/anchor";
import type { Connection, PublicKey } from "@solana/web3.js";
import type { AssetConfigAccount, StrategyAccount } from "../idl/vaultStrategy";
import { MAX_PRICE_AGE_SECONDS, PYTH_PRICE_PRECISION, STRATEGY_INDEX } from "./config";
import { assetConfigPda, shareMintPda, strategyPda, userAta, vaultAta } from "./pdas";
import type { VaultProgram } from "./program";
import { parsePriceUpdateV2, readTokenAmount } from "./pyth";

const toBig = (v: BN): bigint => BigInt(v.toString());
const nowSeconds = (): number => Math.floor(Date.now() / 1000);

export interface AssetView {
  index: number;
  config: PublicKey;
  mint: PublicKey;
  vault: PublicKey;
  priceFeed: PublicKey;
  weightBps: number;
  vaultAmount: bigint;
  price: bigint | null; // exponent -8
  publishTime: number | null;
  stale: boolean;
  valueUsdc: bigint | null; // vaultAmount * price / 1e8
  actualWeight: number | null; // valueUsdc / nav, 0..1
}

export interface StrategyView {
  exists: boolean;
  index: bigint;
  strategy: PublicKey;
  shareMint: PublicKey;
  usdcVault: PublicKey;
  account: StrategyAccount | null;
  usdcAmount: bigint;
  assets: AssetView[];
  navMinor: bigint;
  /** False when a held asset could not be freshly priced (NAV is then a floor). */
  navComplete: boolean;
  totalShares: bigint;
  /** USDC per whole share, scaled by 1e6 (so 1.01 USDC/share → 1_010_000n). */
  navPerShareMinor: bigint;
  fullyAllocated: boolean;
}

export interface Position {
  shares: bigint;
  ownership: number; // 0..1
  valueMinor: bigint; // USDC minor units
  shareAccount: PublicKey;
  shareAccountExists: boolean;
}

/** Fetch just the Strategy account (null if it doesn't exist on this cluster). */
export async function loadStrategyAccount(
  program: VaultProgram,
  index: bigint = STRATEGY_INDEX,
): Promise<{ strategy: PublicKey; account: StrategyAccount } | null> {
  const strategy = strategyPda(index);
  const account = (await program.account.strategy.fetchNullable(strategy)) as StrategyAccount | null;
  return account ? { strategy, account } : null;
}

/**
 * Load everything the UI needs about a strategy: config, assets, vault balances, and
 * freshly parsed oracle prices, then derive NAV exactly as the program does
 * (value = amount * price / 1e8, all in USDC minor units).
 */
export async function loadStrategyView(
  connection: Connection,
  program: VaultProgram,
  index: bigint = STRATEGY_INDEX,
): Promise<StrategyView> {
  const strategy = strategyPda(index);
  const shareMint = shareMintPda(strategy);
  const account = (await program.account.strategy.fetchNullable(strategy)) as StrategyAccount | null;

  if (!account) {
    return {
      exists: false,
      index,
      strategy,
      shareMint,
      usdcVault: shareMint, // placeholder; unused when !exists
      account: null,
      usdcAmount: 0n,
      assets: [],
      navMinor: 0n,
      navComplete: false,
      totalShares: 0n,
      navPerShareMinor: 1_000_000n,
      fullyAllocated: false,
    };
  }

  const usdcVault = vaultAta(account.usdcMint, strategy);
  const assetCount = account.assetCount;

  const configPdas = Array.from({ length: assetCount }, (_, i) => assetConfigPda(strategy, i));
  const configs = (await program.account.assetConfig.fetchMultiple(configPdas)) as (AssetConfigAccount | null)[];

  // One RPC round-trip for the USDC vault + every asset vault + every price feed.
  const raw: PublicKey[] = [usdcVault];
  configs.forEach((c) => {
    if (c) raw.push(c.vault, c.priceFeed);
  });
  const infos = await connection.getMultipleAccountsInfo(raw);

  const usdcInfo = infos[0];
  const usdcAmount = usdcInfo ? readTokenAmount(usdcInfo.data) : 0n;

  const now = nowSeconds();
  let navMinor = usdcAmount;
  let navComplete = true;
  let cursor = 1;

  const assets: AssetView[] = configs.map((c, i) => {
    const config = configPdas[i];
    if (!c) {
      navComplete = false;
      return {
        index: i,
        config,
        mint: config,
        vault: config,
        priceFeed: config,
        weightBps: 0,
        vaultAmount: 0n,
        price: null,
        publishTime: null,
        stale: false,
        valueUsdc: null,
        actualWeight: null,
      };
    }
    const vaultInfo = infos[cursor++];
    const feedInfo = infos[cursor++];
    const vaultAmount = vaultInfo ? readTokenAmount(vaultInfo.data) : 0n;

    let price: bigint | null = null;
    let publishTime: number | null = null;
    let stale = false;
    if (feedInfo) {
      try {
        const parsed = parsePriceUpdateV2(feedInfo.data);
        price = parsed.price;
        publishTime = parsed.publishTime;
        stale = now - publishTime > MAX_PRICE_AGE_SECONDS;
      } catch {
        price = null;
      }
    }

    const priced = price !== null && price > 0n;
    const valueUsdc = priced ? (vaultAmount * price!) / PYTH_PRICE_PRECISION : null;
    if (valueUsdc !== null) navMinor += valueUsdc;
    else if (vaultAmount > 0n) navComplete = false; // holding we can't value

    return {
      index: c.index,
      config,
      mint: c.mint,
      vault: c.vault,
      priceFeed: c.priceFeed,
      weightBps: c.weightBps,
      vaultAmount,
      price,
      publishTime,
      stale,
      valueUsdc,
      actualWeight: null, // filled below once nav is known
    };
  });

  for (const a of assets) {
    a.actualWeight = a.valueUsdc !== null && navMinor > 0n ? Number(a.valueUsdc) / Number(navMinor) : null;
  }

  const totalShares = toBig(account.totalShares);
  const navPerShareMinor = totalShares > 0n ? (navMinor * 1_000_000n) / totalShares : 1_000_000n;

  return {
    exists: true,
    index,
    strategy,
    shareMint,
    usdcVault,
    account,
    usdcAmount,
    assets,
    navMinor,
    navComplete,
    totalShares,
    navPerShareMinor,
    fullyAllocated: account.totalWeightBps === 10_000,
  };
}

/** A wallet's position in the strategy: shares held and their current USDC value. */
export async function loadPosition(connection: Connection, view: StrategyView, owner: PublicKey): Promise<Position> {
  const shareAccount = userAta(view.shareMint, owner);
  const info = await connection.getAccountInfo(shareAccount);
  const shares = info ? readTokenAmount(info.data) : 0n;
  const ownership = view.totalShares > 0n ? Number(shares) / Number(view.totalShares) : 0;
  const valueMinor = view.totalShares > 0n ? (shares * view.navMinor) / view.totalShares : 0n;
  return { shares, ownership, valueMinor, shareAccount, shareAccountExists: info !== null };
}
