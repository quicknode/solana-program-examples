// Offline tests for the pull-oracle (PriceUpdateV2) decoder.
//
// The fixture builder below emits the same bytes as build_mock_price_update_account
// in vault-strategy's Rust tests, so a decode failure here means the client and the
// program disagree about the layout they both read.

import assert from "node:assert";
import { describe, test } from "node:test";
import {
  PRICE_UPDATE_V2_DISCRIMINATOR,
  PYTH_VERIFICATION_LEVEL_FULL,
  PYTH_VERIFICATION_LEVEL_PARTIAL,
} from "./constants.js";
import { parsePythPriceUpdateV2Data } from "./pyth-client.js";

interface FixtureOptions {
  price?: bigint;
  confidence?: bigint;
  exponent?: number;
  publishTime?: bigint;
  emaPrice?: bigint;
  emaConfidence?: bigint;
  postedSlot?: bigint;
  feedIdByte?: number;
  verification?: { kind: "full" } | { kind: "partial"; numSignatures: number };
}

function buildPriceUpdateV2(options: FixtureOptions = {}): Uint8Array {
  const {
    price = 25_000_000_000n,
    confidence = 100_000n,
    exponent = -8,
    publishTime = 1_700_000_000n,
    emaPrice = price,
    emaConfidence = 120_000n,
    postedSlot = 1n,
    feedIdByte = 0xef,
    verification = { kind: "full" as const },
  } = options;

  const verificationBytes = verification.kind === "full" ? 1 : 2;
  const bytes = new Uint8Array(8 + 32 + verificationBytes + 84 + 8);
  const view = new DataView(bytes.buffer);

  bytes.set(PRICE_UPDATE_V2_DISCRIMINATOR, 0);
  // write_authority stays zeroed; the decoder only needs to read it back.
  let offset = 40;
  if (verification.kind === "full") {
    view.setUint8(offset++, PYTH_VERIFICATION_LEVEL_FULL);
  } else {
    view.setUint8(offset++, PYTH_VERIFICATION_LEVEL_PARTIAL);
    view.setUint8(offset++, verification.numSignatures);
  }

  bytes.fill(feedIdByte, offset, offset + 32);
  offset += 32;
  view.setBigInt64(offset, price, true);
  view.setBigUint64(offset + 8, confidence, true);
  view.setInt32(offset + 16, exponent, true);
  view.setBigInt64(offset + 20, publishTime, true);
  view.setBigInt64(offset + 28, publishTime - 1n, true);
  view.setBigInt64(offset + 36, emaPrice, true);
  view.setBigUint64(offset + 44, emaConfidence, true);
  view.setBigUint64(offset + 52, postedSlot, true);

  return bytes;
}

describe("parsePythPriceUpdateV2Data", () => {
  test("decodes a fully-verified update", () => {
    const decoded = parsePythPriceUpdateV2Data(buildPriceUpdateV2());

    assert.ok(decoded, "decodes a well-formed account");
    assert.strictEqual(decoded.price, 25_000_000_000n);
    assert.strictEqual(decoded.confidence, 100_000n);
    assert.strictEqual(decoded.exponent, -8);
    assert.strictEqual(decoded.publishTime, 1_700_000_000n);
    assert.strictEqual(decoded.previousPublishTime, 1_699_999_999n);
    assert.strictEqual(decoded.emaPrice, 25_000_000_000n);
    assert.strictEqual(decoded.emaConfidence, 120_000n);
    assert.strictEqual(decoded.postedSlot, 1n);
    assert.deepStrictEqual(decoded.verificationLevel, { kind: "full" });
    assert.strictEqual(decoded.feedId, "ef".repeat(32));
  });

  test("keeps the price as a raw integer rather than scaling it", () => {
    // The program does integer math on exactly this value. Returning a float here
    // would reintroduce rounding the program never performs.
    const decoded = parsePythPriceUpdateV2Data(buildPriceUpdateV2({ price: 25_000_000_001n }));

    assert.ok(decoded);
    assert.strictEqual(typeof decoded.price, "bigint");
    assert.strictEqual(decoded.price, 25_000_000_001n);
  });

  test("shifts every field by one byte for a partially-verified update", () => {
    // Regression test for the borsh enum: Partial carries a num_signatures payload,
    // so assuming Full's fixed offsets would decode a partial update misaligned.
    const decoded = parsePythPriceUpdateV2Data(
      buildPriceUpdateV2({ verification: { kind: "partial", numSignatures: 3 } }),
    );

    assert.ok(decoded, "decodes a partially-verified account");
    assert.deepStrictEqual(decoded.verificationLevel, { kind: "partial", numSignatures: 3 });
    assert.strictEqual(decoded.price, 25_000_000_000n, "price is still read correctly");
    assert.strictEqual(decoded.exponent, -8);
    assert.strictEqual(decoded.postedSlot, 1n);
  });

  test("decodes a negative price", () => {
    const decoded = parsePythPriceUpdateV2Data(buildPriceUpdateV2({ price: -5n }));

    assert.ok(decoded);
    assert.strictEqual(decoded.price, -5n, "price is signed");
  });

  test("returns null for a non-PriceUpdateV2 account", () => {
    const bytes = buildPriceUpdateV2();
    bytes[0] ^= 0xff;

    assert.strictEqual(parsePythPriceUpdateV2Data(bytes), null);
  });

  test("returns null for a legacy push-oracle account", () => {
    // The push-oracle magic is not a PriceUpdateV2 discriminator, so the two decoders
    // never silently accept each other's accounts.
    const pushOracleAccount = new Uint8Array(132);
    new DataView(pushOracleAccount.buffer).setUint32(0, 0xa1b2c3d4, true);

    assert.strictEqual(parsePythPriceUpdateV2Data(pushOracleAccount), null);
  });

  test("returns null for a truncated account", () => {
    const truncated = buildPriceUpdateV2().subarray(0, 100);

    assert.strictEqual(parsePythPriceUpdateV2Data(truncated), null);
  });

  test("returns null for an unknown verification level", () => {
    const bytes = buildPriceUpdateV2();
    bytes[40] = 9;

    assert.strictEqual(parsePythPriceUpdateV2Data(bytes), null);
  });
});
