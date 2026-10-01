import type { BN } from "@coral-xyz/anchor";
import type { Connection, PublicKey } from "@solana/web3.js";
import type { AssetConfigAccount, FundAccount } from "../idl/managedFund";
import { BPS_DENOMINATOR, FUND_INDEX, MAX_PRICE_AGE_SECONDS } from "./config";
import { formatBps } from "./format";
import { assetConfigPda, fundPda, shareMintPda, userAta, vaultAta } from "./pdas";
import type { FundProgram } from "./program";
import { parsePriceUpdateV2, readTokenAmount } from "./pyth";

const toBig = (v: BN): bigint => BigInt(v.toString());
const nowSeconds = (): number => Math.floor(Date.now() / 1000);

export interface AssetView {
  index: number;
  config: PublicKey;
  mint: PublicKey;
  /** The asset mint's decimals, as recorded on its AssetConfig. */
  decimals: number;
  vault: PublicKey;
  priceFeed: PublicKey;
  weightBps: number;
  vaultAmount: bigint;
  price: bigint | null; // price * 10^exponent dollars per whole token
  exponent: number | null; // read from the feed
  publishTime: number | null;
  stale: boolean;
  valueUsdc: bigint | null; // assetValueInUsdc(vaultAmount, ...), USDC minor units
  actualWeight: number | null; // valueUsdc / nav, 0..1
}

// ---- valuation, mirroring programs/managed-fund/src/oracle.rs ----------------

/** `numerator * 10^power / denominator`, floored; a negative power divides by 10^-power. */
function mulPow10Div(numerator: bigint, power: number, denominator: bigint): bigint {
  const scale = 10n ** BigInt(Math.abs(power));
  return power >= 0 ? (numerator * scale) / denominator : numerator / (denominator * scale);
}

/**
 * Value of `amount` asset minor units in USDC minor units, floored:
 * amount * price * 10^(usdcDecimals + exponent - assetDecimals).
 */
export function assetValueInUsdc(
  amount: bigint,
  price: bigint,
  exponent: number,
  assetDecimals: number,
  usdcDecimals: number,
): bigint {
  return mulPow10Div(amount * price, usdcDecimals + exponent - assetDecimals, 1n);
}

/** The inverse: asset minor units that `usdcAmount` USDC minor units buys at the oracle price, floored. */
export function usdcToAssetAmount(
  usdcAmount: bigint,
  price: bigint,
  exponent: number,
  assetDecimals: number,
  usdcDecimals: number,
): bigint {
  return mulPow10Div(usdcAmount, assetDecimals - exponent - usdcDecimals, price);
}

export interface FundView {
  exists: boolean;
  index: bigint;
  fund: PublicKey;
  shareMint: PublicKey;
  usdcVault: PublicKey;
  account: FundAccount | null;
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

/** Fetch just the Fund account (null if it doesn't exist on this cluster). */
export async function loadFundAccount(
  program: FundProgram,
  index: bigint = FUND_INDEX,
): Promise<{ fund: PublicKey; account: FundAccount } | null> {
  const fund = fundPda(index);
  const account = (await program.account.fund.fetchNullable(fund)) as FundAccount | null;
  return account ? { fund, account } : null;
}

/**
 * Load everything the UI needs about a fund: config, assets, the holdings the
 * program has recorded, and freshly parsed oracle prices, then derive NAV exactly as
 * the program does (each asset valued by `assetValueInUsdc` with its own decimals and
 * its feed's exponent, all in USDC minor units). The
 * program prices shares from its recorded holdings, not the vaults' token balances,
 * so tokens donated straight into a vault are not part of the fund; neither are they
 * here.
 */
export async function loadFundView(
  connection: Connection,
  program: FundProgram,
  index: bigint = FUND_INDEX,
): Promise<FundView> {
  const fund = fundPda(index);
  const shareMint = shareMintPda(fund);
  const account = (await program.account.fund.fetchNullable(fund)) as FundAccount | null;

  if (!account) {
    return {
      exists: false,
      index,
      fund,
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

  const usdcVault = vaultAta(account.usdcMint, fund);
  const assetCount = account.assetCount;

  const configPdas = Array.from({ length: assetCount }, (_, i) => assetConfigPda(fund, i));
  const configs = (await program.account.assetConfig.fetchMultiple(configPdas)) as (AssetConfigAccount | null)[];

  // One RPC round-trip for every price feed.
  const raw: PublicKey[] = [];
  configs.forEach((c) => {
    if (c) raw.push(c.priceFeed);
  });
  const infos = await connection.getMultipleAccountsInfo(raw);

  const usdcAmount = toBig(account.usdcHoldings);

  const now = nowSeconds();
  let navMinor = usdcAmount;
  let navComplete = true;
  let cursor = 0;

  const assets: AssetView[] = configs.map((c, i) => {
    const config = configPdas[i];
    if (!c) {
      navComplete = false;
      return {
        index: i,
        config,
        mint: config,
        decimals: 0,
        vault: config,
        priceFeed: config,
        weightBps: 0,
        vaultAmount: 0n,
        price: null,
        exponent: null,
        publishTime: null,
        stale: false,
        valueUsdc: null,
        actualWeight: null,
      };
    }
    const feedInfo = infos[cursor++];
    const vaultAmount = toBig(account.assetHoldings[c.index]);

    let price: bigint | null = null;
    let exponent: number | null = null;
    let publishTime: number | null = null;
    let stale = false;
    if (feedInfo) {
      try {
        const parsed = parsePriceUpdateV2(feedInfo.data);
        price = parsed.price;
        exponent = parsed.exponent;
        publishTime = parsed.publishTime;
        stale = now - publishTime > MAX_PRICE_AGE_SECONDS;
      } catch {
        price = null;
      }
    }

    const valueUsdc =
      price !== null && price > 0n && exponent !== null
        ? assetValueInUsdc(vaultAmount, price, exponent, c.decimals, account.usdcDecimals)
        : null;
    if (valueUsdc !== null) navMinor += valueUsdc;
    else if (vaultAmount > 0n) navComplete = false; // holding we can't value

    return {
      index: c.index,
      config,
      mint: c.mint,
      decimals: c.decimals,
      vault: c.vault,
      priceFeed: c.priceFeed,
      weightBps: c.weightBps,
      vaultAmount,
      price,
      exponent,
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
    fund,
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

// ---- rebalance, mirroring programs/managed-fund/src/instructions/rebalance.rs ----

/** An asset's distance from its target, in USDC minor units and in bps of NAV (positive = over). */
export interface AssetDrift {
  targetUsdc: bigint;
  driftUsdc: bigint;
  driftBps: number;
}

export function assetDrift(view: FundView, asset: AssetView): AssetDrift | null {
  if (asset.valueUsdc === null || view.navMinor === 0n) return null;
  const targetUsdc = (view.navMinor * BigInt(asset.weightBps)) / BigInt(BPS_DENOMINATOR);
  const driftUsdc = asset.valueUsdc - targetUsdc;
  return { targetUsdc, driftUsdc, driftBps: Number((driftUsdc * BigInt(BPS_DENOMINATOR)) / view.navMinor) };
}

export interface RebalancePlan {
  /** Why the program would refuse this pair, or null when it would trade. */
  block: string | null;
  /** USDC value the program would move: the smaller of the sell excess and the buy shortfall. */
  tradeUsdc: bigint;
  /** Asset minor units the program would sell. */
  sellAmount: bigint;
}

/**
 * What `rebalance(sell, buy)` would do against this view. The program computes the trade
 * itself; this repeats its checks so the UI can say in advance whether a pair qualifies.
 */
export function rebalancePlan(view: FundView, sellIndex: number, buyIndex: number): RebalancePlan {
  const none = (block: string): RebalancePlan => ({ block, tradeUsdc: 0n, sellAmount: 0n });
  const s = view.account;
  const sell = view.assets[sellIndex];
  const buy = view.assets[buyIndex];
  if (!s || !sell || !buy) return none("Choose two assets.");
  if (sellIndex === buyIndex) return none("Choose two different assets.");
  // The program prices every asset, not just the pair, so any stale feed reverts it.
  if (!view.navComplete || view.assets.some((a) => a.price === null || a.exponent === null || a.stale)) {
    return none("An oracle price is stale or missing — the rebalance would revert on-chain.");
  }
  const sellDrift = assetDrift(view, sell);
  const buyDrift = assetDrift(view, buy);
  if (!sellDrift || !buyDrift) return none("The fund has no value to rebalance.");

  const excess = sellDrift.driftUsdc > 0n ? sellDrift.driftUsdc : 0n;
  const threshold = (view.navMinor * BigInt(s.rebalanceThresholdBps)) / BigInt(BPS_DENOMINATOR);
  if (excess === 0n || (sell.weightBps !== 0 && excess < threshold)) {
    return none(
      `#${sell.index} is not over its target weight by the fund's ${formatBps(s.rebalanceThresholdBps)} threshold.`,
    );
  }
  const shortfall = buyDrift.driftUsdc < 0n ? -buyDrift.driftUsdc : 0n;
  if (shortfall === 0n) return none(`#${buy.index} is not below its target weight.`);

  const tradeUsdc = excess < shortfall ? excess : shortfall;
  const sellAmount = usdcToAssetAmount(tradeUsdc, sell.price!, sell.exponent!, sell.decimals, s.usdcDecimals);
  if (sellAmount === 0n) return none("The trade would sell none of the asset.");
  if (sellAmount > sell.vaultAmount) return none("The trade would sell more than the fund holds.");
  return { block: null, tradeUsdc, sellAmount };
}

/** A wallet's position in the fund: shares held and their current USDC value. */
export async function loadPosition(connection: Connection, view: FundView, owner: PublicKey): Promise<Position> {
  const shareAccount = userAta(view.shareMint, owner);
  const info = await connection.getAccountInfo(shareAccount);
  const shares = info ? readTokenAmount(info.data) : 0n;
  const ownership = view.totalShares > 0n ? Number(shares) / Number(view.totalShares) : 0;
  const valueMinor = view.totalShares > 0n ? (shares * view.navMinor) / view.totalShares : 0n;
  return { shares, ownership, valueMinor, shareAccount, shareAccountExists: info !== null };
}
