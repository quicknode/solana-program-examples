import type { BN } from "@coral-xyz/anchor";
import type { Connection, PublicKey } from "@solana/web3.js";
import type { AssetConfigAccount, FundAccount } from "../idl/managedFund";
import { FUND_INDEX, MAX_PRICE_AGE_SECONDS, PYTH_PRICE_PRECISION } from "./config";
import { assetConfigPda, fundPda, shareMintPda, userAta, vaultAta } from "./pdas";
import type { FundProgram } from "./program";
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
 * the program does (value = amount * price / 1e8, all in USDC minor units). The
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
    const feedInfo = infos[cursor++];
    const vaultAmount = toBig(account.assetHoldings[c.index]);

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

/** A wallet's position in the fund: shares held and their current USDC value. */
export async function loadPosition(connection: Connection, view: FundView, owner: PublicKey): Promise<Position> {
  const shareAccount = userAta(view.shareMint, owner);
  const info = await connection.getAccountInfo(shareAccount);
  const shares = info ? readTokenAmount(info.data) : 0n;
  const ownership = view.totalShares > 0n ? Number(shares) / Number(view.totalShares) : 0;
  const valueMinor = view.totalShares > 0n ? (shares * view.navMinor) / view.totalShares : 0n;
  return { shares, ownership, valueMinor, shareAccount, shareAccountExists: info !== null };
}
