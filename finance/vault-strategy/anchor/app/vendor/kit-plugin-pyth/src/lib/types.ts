import type { Address, TransactionSendingSigner } from "@solana/kit";
import type { Connection } from "solana-kite";

export interface PythPrice {
  /** The price as a floating-point number (raw_price * 10^exponent) */
  price: number;
  /** The confidence interval as a floating-point number */
  confidence: number;
  /** The exponent used to scale from raw integer to float */
  exponent: number;
  /** Unix timestamp in seconds when this price was published */
  publishTime: number;
}

export interface PythPriceFeed {
  /** The 32-byte feed ID as a lowercase hex string (without 0x prefix) */
  id: string;
  /** The current spot price */
  price: PythPrice;
  /** The exponential moving average price */
  emaPrice: PythPrice;
}

export type PythPriceStatus = "trading" | "halted" | "auction" | "unknown";

export interface PythOnchainPriceData {
  price: number;
  confidence: number;
  exponent: number;
  emaPrice: number;
  emaConfidence: number;
  status: PythPriceStatus;
  /** Unix timestamp in seconds when this price was last published */
  publishTime: bigint;
  /** Solana slot when this price was last published */
  slot: bigint;
}

export type PythVerificationLevel = { kind: "partial"; numSignatures: number } | { kind: "full" };

/**
 * A decoded pull-oracle PriceUpdateV2 account — the format onchain programs read
 * via pyth-solana-receiver-sdk, and the format postPythPriceUpdate creates.
 *
 * Every price field is the raw published integer at 10^exponent, not a float.
 * A program consuming these accounts does integer math on exactly these values,
 * so a client that wants to agree with it must do the same; converting to a
 * JavaScript number here would silently introduce rounding the program never has.
 * Scale for display only, at the edge: Number(price) * 10 ** exponent.
 */
export interface PythPriceUpdateV2 {
  /** 32-byte feed id as lowercase hex, matching the ids Hermes uses. */
  feedId: string;
  writeAuthority: Address;
  verificationLevel: PythVerificationLevel;
  price: bigint;
  confidence: bigint;
  exponent: number;
  /** Unix seconds. Pyth publishes a wall-clock time, not a slot. */
  publishTime: bigint;
  previousPublishTime: bigint;
  emaPrice: bigint;
  emaConfidence: bigint;
  postedSlot: bigint;
}

export interface PythFeedInfo {
  id: string;
  attributes: {
    asset_type: string;
    base: string;
    description: string;
    generic_symbol?: string;
    quote_currency: string;
    symbol: string;
    tenor?: string;
  };
}

export interface PythConfig {
  /** Base URL for the Pyth Hermes API. Defaults to https://hermes.pyth.network */
  hermesUrl?: string;
}

export type PythPriceCallback = (error: Error | null, feed: PythPriceFeed | null) => void;

export interface PythMethods {
  /** Fetch the latest price for a single feed from the Hermes API */
  getPythPriceFeed(feedId: string): Promise<PythPriceFeed | null>;
  /** Fetch the latest prices for multiple feeds in one request */
  getPythPriceFeeds(feedIds: Array<string>): Promise<Map<string, PythPriceFeed>>;
  /**
   * Decode a legacy push-oracle Pyth price account from onchain state. Live mainnet prices
   * come from the pull oracle (getPythPriceFeed / postPythPriceUpdate); this decodes
   * existing/snapshotted push-oracle accounts. Returns null if the account does not exist.
   */
  getPythOnchainPrice(priceAccountAddress: Address): Promise<PythOnchainPriceData | null>;
  /**
   * Decode a pull-oracle PriceUpdateV2 account — what postPythPriceUpdate creates and
   * what onchain programs read. Returns null if the account does not exist or does not
   * carry the PriceUpdateV2 discriminator.
   */
  getPythPriceUpdateV2(priceUpdateAddress: Address): Promise<PythPriceUpdateV2 | null>;
  /** Returns true if the feed's last publish time exceeds maxAgeSeconds */
  isPythPriceStale(feedId: string, maxAgeSeconds: number): Promise<boolean>;
  /** Search Pyth's feed catalogue by name or symbol */
  searchPythFeeds(query: string, assetType?: string): Promise<Array<PythFeedInfo>>;
  /** Subscribe to price updates, calling callback on each poll */
  watchPythPriceFeed(feedId: string, callback: PythPriceCallback, intervalMs?: number): () => void;
  /**
   * Post a single Pyth pull-oracle price update onchain via the Pyth Receiver program.
   * Returns the address of the temporary PriceUpdateV2 account that was created.
   * Call reclaimPythPriceUpdateRent() to close the account and recover rent when done.
   */
  postPythPriceUpdate(feedId: string, payer: TransactionSendingSigner): Promise<Address>;
  /**
   * Post multiple Pyth pull-oracle price updates onchain in parallel (one transaction per feed).
   * Returns the addresses of the temporary PriceUpdateV2 accounts in the same order as feedIds.
   */
  postPythPriceUpdates(feedIds: Array<string>, payer: TransactionSendingSigner): Promise<Array<Address>>;
  /**
   * Close a temporary PriceUpdateV2 account and reclaim its rent.
   * Call this after your program has consumed the price data.
   */
  reclaimPythPriceUpdateRent(priceUpdateAccount: Address, payer: TransactionSendingSigner): Promise<string>;
}

export type ConnectionWithPyth = Connection & {
  pyth: PythMethods;
};
