import assert from "node:assert";
import { existsSync, readFileSync } from "node:fs";
import { describe, test } from "node:test";
import { isAddress } from "@solana/kit";
import { NATIVE_MINT_ADDRESS, POPULAR_TOKENS } from "../src/components/token-picker/popular-token-addresses";

const sourceFile = readFileSync(
  new URL("../src/components/token-picker/popular-token-addresses.ts", import.meta.url),
  "utf8",
);

describe("popular token addresses", () => {
  for (const [cluster, tokens] of Object.entries(POPULAR_TOKENS)) {
    test(`${cluster}: every address is a Solana address, listed once, with an https source`, () => {
      for (const token of tokens) {
        assert.ok(isAddress(token.address), `${token.symbol} ${token.address}`);
        assert.match(token.source, /^https:\/\/\S+$/, `${token.symbol} has no source`);
      }
      const addresses = tokens.map((token) => token.address);
      assert.strictEqual(new Set(addresses).size, addresses.length, "an address is listed twice");
      const symbols = tokens.map((token) => token.symbol);
      assert.strictEqual(new Set(symbols).size, symbols.length, "a symbol is listed twice");
    });
  }

  test("only devnet and mainnet have presets", () => {
    assert.deepStrictEqual(POPULAR_TOKENS.localnet, []);
    assert.deepStrictEqual(POPULAR_TOKENS.testnet, []);
  });

  test("the comment above every address cites an https page", () => {
    const lines = sourceFile.split("\n");
    lines.forEach((line, index) => {
      if (!/address: "|ADDRESS = "/.test(line)) {
        return;
      }
      // The proof comment is the run of // lines just above the address, or
      // above the object the address opens.
      let cursor = index - 1;
      const comment: Array<string> = [];
      while (cursor >= 0 && lines[cursor].trim().startsWith("//")) {
        comment.unshift(lines[cursor]);
        cursor -= 1;
      }
      assert.match(comment.join("\n"), /https:\/\//, `no proof comment above line ${index + 1}: ${line.trim()}`);
    });
  });

  test("every bundled logo exists in public/", () => {
    for (const token of Object.values(POPULAR_TOKENS).flat()) {
      if (token.logo) {
        assert.ok(existsSync(new URL(`../public${token.logo}`, import.meta.url)), `${token.logo} is missing`);
      }
    }
  });

  test("SOL is the token program's native mint on both clusters", () => {
    for (const cluster of ["devnet", "mainnet"] as const) {
      assert.strictEqual(POPULAR_TOKENS[cluster][0].address, NATIVE_MINT_ADDRESS);
    }
  });
});
