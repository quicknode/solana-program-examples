import assert from "node:assert";
import { describe, test } from "node:test";
import { AmountError, formatAmount, formatPrice, parseAmount, unitPrice } from "../src/lib/amounts";

describe("parseAmount", () => {
  test("turns decimal text into the token's smallest units", () => {
    assert.strictEqual(parseAmount("250", 6), 250_000_000n);
    assert.strictEqual(parseAmount("250.5", 6), 250_500_000n);
    assert.strictEqual(parseAmount("0.00000001", 8), 1n);
    assert.strictEqual(parseAmount(".5", 6), 500_000n);
    assert.strictEqual(parseAmount("1,000", 6), 1_000_000_000n);
  });

  test("refuses more decimal places than the token has", () => {
    assert.throws(() => parseAmount("0.0000001", 6), AmountError);
    assert.throws(() => parseAmount("1.5", 0), AmountError);
  });

  test("refuses text that is not a number, and amounts over u64", () => {
    assert.throws(() => parseAmount("", 6), AmountError);
    assert.throws(() => parseAmount("abc", 6), AmountError);
    assert.throws(() => parseAmount("-1", 6), AmountError);
    assert.throws(() => parseAmount("18446744073709551616", 0), AmountError);
    assert.strictEqual(parseAmount("18446744073709551615", 0), 2n ** 64n - 1n);
  });
});

describe("formatAmount", () => {
  test("shows smallest units as decimal text, grouping thousands and trimming zeros", () => {
    assert.strictEqual(formatAmount(250_000_000n, 6), "250");
    assert.strictEqual(formatAmount(1_250_500_000n, 6), "1,250.5");
    assert.strictEqual(formatAmount(1n, 8), "0.00000001");
    assert.strictEqual(formatAmount(0n, 6), "0");
    assert.strictEqual(formatAmount(2n ** 64n - 1n, 0), "18,446,744,073,709,551,615");
  });

  test("round-trips with parseAmount", () => {
    for (const text of ["0.004", "250", "1234567.891011"]) {
      assert.strictEqual(formatAmount(parseAmount(text, 8), 8).replaceAll(",", ""), text);
    }
  });
});

describe("unitPrice", () => {
  test("prices one whole base token in the quote token, across different decimals", () => {
    // 250 USDC (6 decimals) for 1 TSLAx (8 decimals).
    assert.strictEqual(unitPrice(100_000_000n, 8, 250_000_000n, 6), 250);
    assert.strictEqual(unitPrice(250_000_000n, 6, 100_000_000n, 8), 0.004);
    assert.strictEqual(formatPrice(unitPrice(3n, 0, 1n, 0)), "0.333333");
  });

  test("an empty side has no price", () => {
    assert.strictEqual(unitPrice(0n, 6, 1n, 6), Number.POSITIVE_INFINITY);
    assert.strictEqual(formatPrice(Number.POSITIVE_INFINITY), "-");
  });
});
