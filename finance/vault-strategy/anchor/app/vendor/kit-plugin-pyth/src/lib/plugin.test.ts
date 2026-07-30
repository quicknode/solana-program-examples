import assert from "node:assert";
import { describe, test } from "node:test";
import { address } from "@solana/kit";
import { connect } from "solana-kite";
import { PYTH_ACCOUNT_TYPE_PRICE, PYTH_FEED_IDS, PYTH_PRICE_ACCOUNT_MAGIC } from "./constants.js";
import { pyth as pythPlugin } from "./plugin.js";
import { parsePythPriceAccountData } from "./pyth-client.js";

describe("pythPlugin", () => {
  test("plugin returns object with all pyth methods", () => {
    const connection = connect("devnet");
    const result = pythPlugin()(connection);

    assert.ok(typeof result.pyth.getPythPriceFeed === "function");
    assert.ok(typeof result.pyth.getPythPriceFeeds === "function");
    assert.ok(typeof result.pyth.getPythOnchainPrice === "function");
    assert.ok(typeof result.pyth.isPythPriceStale === "function");
    assert.ok(typeof result.pyth.searchPythFeeds === "function");
    assert.ok(typeof result.pyth.watchPythPriceFeed === "function");
    assert.ok(typeof result.pyth.postPythPriceUpdate === "function");
    assert.ok(typeof result.pyth.postPythPriceUpdates === "function");
    assert.ok(typeof result.pyth.reclaimPythPriceUpdateRent === "function");
  });
});

describe("getPythPriceFeed integration", () => {
  test("fetches SOL/USD price from Hermes", async () => {
    const connection = connect("mainnet-beta");
    const { pyth } = pythPlugin()(connection);

    const feed = await pyth.getPythPriceFeed(PYTH_FEED_IDS.SOL_USD);

    assert.ok(feed, "Feed should exist");
    assert.ok(feed.price.price > 0, "Price should be positive");
    assert.ok(feed.price.confidence > 0, "Confidence should be positive");
    assert.ok(typeof feed.price.exponent === "number", "Exponent should be a number");
    assert.ok(feed.price.publishTime > 0, "Publish time should be set");
    assert.ok(feed.emaPrice.price > 0, "EMA price should be positive");
    assert.strictEqual(feed.id, PYTH_FEED_IDS.SOL_USD, "Feed ID should match");
  });

  test("accepts feed IDs with 0x prefix", async () => {
    const connection = connect("mainnet-beta");
    const { pyth } = pythPlugin()(connection);

    const feedWithPrefix = await pyth.getPythPriceFeed(`0x${PYTH_FEED_IDS.SOL_USD}`);
    const feedWithout = await pyth.getPythPriceFeed(PYTH_FEED_IDS.SOL_USD);

    assert.ok(feedWithPrefix, "Should accept 0x prefix");
    assert.ok(feedWithout, "Should work without prefix");
    assert.strictEqual(feedWithPrefix!.id, feedWithout!.id, "IDs should match regardless of prefix");
  });
});

describe("getPythPriceFeeds integration", () => {
  test("fetches multiple prices in a single request", async () => {
    const connection = connect("mainnet-beta");
    const { pyth } = pythPlugin()(connection);

    const feeds = await pyth.getPythPriceFeeds([PYTH_FEED_IDS.SOL_USD, PYTH_FEED_IDS.BTC_USD]);

    assert.strictEqual(feeds.size, 2, "Should return 2 feeds");
    assert.ok(feeds.has(PYTH_FEED_IDS.SOL_USD), "Should have SOL/USD feed");
    assert.ok(feeds.has(PYTH_FEED_IDS.BTC_USD), "Should have BTC/USD feed");

    const solFeed = feeds.get(PYTH_FEED_IDS.SOL_USD)!;
    assert.ok(solFeed.price.price > 0, "SOL price should be positive");

    const btcFeed = feeds.get(PYTH_FEED_IDS.BTC_USD)!;
    assert.ok(btcFeed.price.price > solFeed.price.price, "BTC should cost more than SOL");
  });
});

function buildMockPythPriceAccount({
  exponent,
  priceRaw,
  confRaw,
  emaPriceRaw,
  emaConfRaw,
  timestamp,
  statusCode,
  slot,
}: {
  exponent: number;
  priceRaw: bigint;
  confRaw: bigint;
  emaPriceRaw: bigint;
  emaConfRaw: bigint;
  timestamp: bigint;
  statusCode: number;
  slot: bigint;
}): Uint8Array {
  const buffer = new ArrayBuffer(132);
  const view = new DataView(buffer);
  view.setUint32(0, PYTH_PRICE_ACCOUNT_MAGIC, true);
  view.setUint32(8, PYTH_ACCOUNT_TYPE_PRICE, true);
  view.setInt32(20, exponent, true);
  view.setBigInt64(48, emaPriceRaw, true);
  view.setBigUint64(56, emaConfRaw, true);
  view.setBigInt64(64, timestamp, true);
  view.setBigInt64(100, priceRaw, true);
  view.setBigUint64(108, confRaw, true);
  view.setUint32(116, statusCode, true);
  view.setBigUint64(124, slot, true);
  return new Uint8Array(buffer);
}

describe("parsePythPriceAccountData", () => {
  test("parses valid mock price account data", () => {
    const mockData = buildMockPythPriceAccount({
      exponent: -2,
      priceRaw: BigInt(15000),
      confRaw: BigInt(75),
      emaPriceRaw: BigInt(14900),
      emaConfRaw: BigInt(50),
      timestamp: BigInt(1700000000),
      statusCode: 1,
      slot: BigInt(250000000),
    });

    const result = parsePythPriceAccountData(mockData);

    assert.ok(result, "Should parse valid data");
    assert.strictEqual(result.price, 150.0, "Price should be 15000 * 10^-2 = 150.00");
    assert.strictEqual(result.confidence, 0.75, "Confidence should be 75 * 10^-2 = 0.75");
    assert.strictEqual(result.emaPrice, 149.0, "EMA price should be 14900 * 10^-2 = 149.00");
    assert.strictEqual(result.emaConfidence, 0.5, "EMA confidence should be 50 * 10^-2 = 0.50");
    assert.strictEqual(result.exponent, -2, "Exponent should match");
    assert.strictEqual(result.status, "trading", "Status code 1 should map to trading");
    assert.strictEqual(result.slot, BigInt(250000000), "Slot should match");
    assert.strictEqual(result.publishTime, BigInt(1700000000), "Publish time should match");
  });

  test("returns null for data that is too short", () => {
    const tooShort = new Uint8Array(10);
    assert.strictEqual(parsePythPriceAccountData(tooShort), null);
  });

  test("returns null for wrong magic number", () => {
    const mockData = buildMockPythPriceAccount({
      exponent: -2,
      priceRaw: BigInt(15000),
      confRaw: BigInt(75),
      emaPriceRaw: BigInt(14900),
      emaConfRaw: BigInt(50),
      timestamp: BigInt(1700000000),
      statusCode: 1,
      slot: BigInt(250000000),
    });
    // Corrupt the magic number
    new DataView(mockData.buffer).setUint32(0, 0x12345678, true);
    assert.strictEqual(parsePythPriceAccountData(mockData), null);
  });

  test("returns null for wrong account type", () => {
    const mockData = buildMockPythPriceAccount({
      exponent: -2,
      priceRaw: BigInt(15000),
      confRaw: BigInt(75),
      emaPriceRaw: BigInt(14900),
      emaConfRaw: BigInt(50),
      timestamp: BigInt(1700000000),
      statusCode: 1,
      slot: BigInt(250000000),
    });
    // Set account type to something other than PTYPE_PRICE (3)
    new DataView(mockData.buffer).setUint32(8, 1, true);
    assert.strictEqual(parsePythPriceAccountData(mockData), null);
  });
});

describe("isPythPriceStale integration", () => {
  test("SOL/USD price is not stale with 60-second max age", async () => {
    const connection = connect("mainnet-beta");
    const { pyth } = pythPlugin()(connection);

    // Pyth publishes every ~400ms so 60 seconds is very generous
    const isStale = await pyth.isPythPriceStale(PYTH_FEED_IDS.SOL_USD, 60);

    assert.strictEqual(isStale, false, "Price should be fresh");
  });

  test("price is considered stale with 0-second max age", async () => {
    const connection = connect("mainnet-beta");
    const { pyth } = pythPlugin()(connection);

    const isStale = await pyth.isPythPriceStale(PYTH_FEED_IDS.SOL_USD, 0);

    assert.strictEqual(isStale, true, "Price should be stale with 0s max age");
  });
});

describe("searchPythFeeds integration", () => {
  test("finds BTC crypto feeds by query", async () => {
    const connection = connect("mainnet-beta");
    const { pyth } = pythPlugin()(connection);

    const feeds = await pyth.searchPythFeeds("BTC/USD", "crypto");

    assert.ok(feeds.length > 0, "Should find at least one BTC feed");
    for (const feed of feeds) {
      assert.strictEqual(feed.attributes.asset_type, "Crypto", "All results should be Crypto type");
    }
  });

  test("returned feed objects have id and attributes", async () => {
    const connection = connect("mainnet-beta");
    const { pyth } = pythPlugin()(connection);

    const feeds = await pyth.searchPythFeeds("SOL/USD");

    assert.ok(feeds.length > 0, "Should find SOL feeds");
    const firstFeed = feeds[0]!;
    assert.ok(typeof firstFeed.id === "string", "Feed should have an id");
    assert.ok(!firstFeed.id.startsWith("0x"), "Feed ID should not have 0x prefix");
    assert.ok(typeof firstFeed.attributes.symbol === "string", "Feed should have a symbol");
  });
});

describe("watchPythPriceFeed", () => {
  test("polls price and invokes callback with feed data", async () => {
    const connection = connect("mainnet-beta");
    const { pyth } = pythPlugin()(connection);

    let callbackCount = 0;
    let lastFeed: unknown = null;

    const stopWatching = pyth.watchPythPriceFeed(
      PYTH_FEED_IDS.SOL_USD,
      (error, feed) => {
        if (error) assert.fail(`Unexpected error: ${error.message}`);
        callbackCount++;
        lastFeed = feed;
      },
      500,
    );

    await new Promise((resolve) => setTimeout(resolve, 1500));
    stopWatching();

    assert.ok(callbackCount >= 1, "Callback should have been invoked at least once");
    assert.ok(lastFeed !== null, "Should have received a feed");
    assert.ok((lastFeed as { price: { price: number } }).price.price > 0, "Feed price should be positive");
  });
});
