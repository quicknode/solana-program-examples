// Offline tests: plugin construction and the pure onchain price-account
// decoder. Run in CI without network access. (Network/Hermes integration
// tests live in plugin.test.ts and run via `npm test`.)

import assert from "node:assert";
import { describe, test } from "node:test";
import { connect } from "solana-kite";
import { PYTH_ACCOUNT_TYPE_PRICE, PYTH_PRICE_ACCOUNT_MAGIC } from "./constants.js";
import { pyth } from "./plugin.js";
import { parsePythPriceAccountData } from "./pyth-client.js";

describe("pyth() plugin", () => {
  test("extends a client with the pyth namespace (offline construction)", () => {
    const client = pyth()(connect("devnet"));

    assert.ok(client.pyth, "exposes the pyth namespace");
    assert.strictEqual(typeof client.pyth.getPythPriceFeed, "function");
    assert.strictEqual(typeof client.pyth.getPythOnchainPrice, "function");
    assert.strictEqual(typeof client.pyth.postPythPriceUpdate, "function");
    // Regression test: the plugin must preserve the underlying connection
    // (a previous version returned only `{ pyth }`).
    assert.strictEqual(typeof client.getLamportBalance, "function");
    assert.ok(client.rpc, "preserves the rpc");
  });
});

describe("parsePythPriceAccountData", () => {
  test("decodes a well-formed price account", () => {
    const buffer = new ArrayBuffer(132);
    const view = new DataView(buffer);
    view.setUint32(0, PYTH_PRICE_ACCOUNT_MAGIC, true);
    view.setUint32(8, PYTH_ACCOUNT_TYPE_PRICE, true);
    view.setInt32(20, -2, true); // exponent
    view.setBigInt64(64, 1_700_000_000n, true); // publish time
    view.setBigInt64(100, 15_000n, true); // price
    view.setBigUint64(108, 75n, true); // confidence
    view.setUint32(116, 1, true); // status = trading
    view.setBigUint64(124, 250_000_000n, true); // slot

    const result = parsePythPriceAccountData(new Uint8Array(buffer));

    assert.ok(result, "should decode");
    assert.strictEqual(result.price, 150.0);
    assert.strictEqual(result.status, "trading");
    assert.strictEqual(result.exponent, -2);
  });

  test("rejects data with the wrong magic number", () => {
    const data = new Uint8Array(132);
    new DataView(data.buffer).setUint32(0, 0xdeadbeef, true);
    assert.strictEqual(parsePythPriceAccountData(data), null);
  });
});
