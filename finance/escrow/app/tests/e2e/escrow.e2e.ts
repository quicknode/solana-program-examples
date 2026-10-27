import assert from "node:assert";
import { mkdirSync } from "node:fs";
import { after, before, describe, test as nodeTest } from "node:test";
import type { Address } from "@solana/kit";
import { findAssociatedTokenPda } from "@solana-program/token";
import { getTokenDecoder, TOKEN_2022_PROGRAM_ADDRESS } from "@solana-program/token-2022";
import { type Browser, chromium, type Page } from "playwright";
import {
  ESCROW_PROGRAM_ADDRESS,
  getOfferDecoder,
  getOfferEncoder,
  parseTakeOfferInstruction,
} from "../../src/generated";
import { buildAndStartApp } from "./app-server";
import { buildMarket, MAKERS, type Market, MINTS } from "./market";
import { type MockCluster, startMockCluster } from "./mock-cluster";
import { mockWalletScript } from "./mock-wallet";

// The app, built and served against a mock cluster, driven in Chromium by a
// test wallet that approves everything. The viewer is Bob, who holds 1,000
// tUSDC, 1.5 tTSLAx and 1 SOL. These tests run in order: each starts from
// the market the one before it left.

const CLUSTER_PORT = 18899;
const APP_PORT = 3100;

let market: Market;
let cluster: MockCluster;
let app: { url: string; stop: () => void };
let browser: Browser;
let page: Page;

const balanceOf = async (owner: Address, mint: Address): Promise<bigint | null> => {
  const [tokenAccount] = await findAssociatedTokenPda({ owner, mint, tokenProgram: TOKEN_2022_PROGRAM_ADDRESS });
  const account = cluster.accounts.get(tokenAccount);
  return account ? getTokenDecoder().decode(account.data).amount : null;
};

const offerCards = () => page.locator(".offer-card");

// A test that fails leaves a screenshot of the page in tests/e2e/failures.
const test = (name: string, body: () => Promise<void>) =>
  nodeTest(name, async () => {
    try {
      await body();
    } catch (error) {
      mkdirSync(new URL("failures", import.meta.url), { recursive: true });
      const file = new URL(`failures/${name.replace(/[^a-z0-9]+/gi, "-")}.png`, import.meta.url);
      await page.screenshot({ path: file.pathname, fullPage: true });
      throw error;
    }
  });

before(async () => {
  market = await buildMarket();
  cluster = await startMockCluster(market.accounts, CLUSTER_PORT);
  app = await buildAndStartApp(cluster.rpcUrl, APP_PORT);
  browser = await chromium.launch();
  page = await browser.newPage({ viewport: { width: 1280, height: 900 } });
  page.on("pageerror", (error) => {
    throw error;
  });
  page.on("console", (message) => {
    if (message.type() === "error") {
      console.log(`browser console: ${message.text()}`);
    }
  });
  await page.addInitScript(mockWalletScript(market.bobPrivateKey, market.bob.address, "solana:localnet"));
});

after(async () => {
  await browser?.close();
  app?.stop();
  await cluster?.close();
});

describe("the escrow app", () => {
  test("lists every open offer before a wallet connects", async () => {
    await page.goto(app.url);
    await page.getByText("4 of 4 open offers").waitFor();
    assert.strictEqual(await offerCards().count(), 4);
    assert.strictEqual(await page.locator(".status-tag").count(), 0, "no affordability without a wallet");
  });

  test("connects the wallet and says which offers Bob can pay for", async () => {
    await page.getByRole("button", { name: "Connect wallet" }).click();
    await page.getByRole("button", { name: "Test Wallet" }).click();
    await page.getByText("Your offer").waitFor();

    const aliceCard = offerCards().filter({ hasText: "250 tUSDC" });
    await aliceCard.getByText("You can take this").waitFor();
    // Carol wants 2.5 tTSLAx and Bob holds 1.5.
    await offerCards().filter({ hasText: "500 tUSDC" }).getByText("Need 1 more tTSLAx").waitFor();
  });

  test("filters to one pair, cheapest first", async () => {
    await page.getByLabel("You want to get").selectOption({ label: "tUSDC" });
    await page.getByLabel("You want to pay with").selectOption({ label: "tTSLAx" });
    await page.getByText("2 of 4 open offers").waitFor();
    const first = await offerCards().nth(0).textContent();
    assert.match(first ?? "", /250 tUSDC/, "Alice's 250 tUSDC a tTSLAx beats Carol's 200");

    await page.getByLabel("Only offers I can pay for").check();
    await page.getByText("1 of 4 open offers").waitFor();
    await page.getByLabel("Only offers I can pay for").uncheck();
  });

  test("takes Alice's offer: Bob pays 1 tTSLAx and gets 250 tUSDC", async () => {
    await offerCards().filter({ hasText: "250 tUSDC" }).click();
    await page.getByRole("heading", { name: "Take this offer" }).waitFor();
    await page.getByRole("button", { name: "Pay 1 tTSLAx, get 250 tUSDC" }).click();
    await page.getByRole("heading", { name: "Offer taken" }).waitFor();

    assert.strictEqual(cluster.accounts.has(market.offers.alice), false, "the offer account is closed");
    assert.strictEqual(await balanceOf(market.bob.address, MINTS.usdc.address), 1_250_000_000n);
    assert.strictEqual(await balanceOf(market.bob.address, MINTS.tslax.address), 50_000_000n);
    assert.strictEqual(await balanceOf(MAKERS.alice, MINTS.tslax.address), 100_000_000n);

    const take = cluster.transactions
      .at(-1)
      ?.instructions.find((instruction) => instruction.programAddress === ESCROW_PROGRAM_ADDRESS);
    assert.ok(take);
    const { data } = parseTakeOfferInstruction(take as Parameters<typeof parseTakeOfferInstruction>[0]);
    assert.strictEqual(data.minimumTokenAOut, 250_000_000n, "Bob's bounds are the terms he was shown");
    assert.strictEqual(data.maximumTokenBIn, 100_000_000n);
  });

  test("refuses a take when the maker switched the offer, and moves nothing", async () => {
    await page.goto(`${app.url}/offers/${market.offers.dave}`);
    await page.getByRole("button", { name: "Pay 540 tUSDC, get 3 tNVDAx" }).waitFor();

    // Dave cancels and re-makes the offer under the same id, now wanting 600
    // tUSDC, after Bob loaded the page and before his take lands.
    const account = cluster.accounts.get(market.offers.dave);
    assert.ok(account);
    const offer = getOfferDecoder().decode(account.data);
    cluster.accounts.set(market.offers.dave, {
      ...account,
      data: getOfferEncoder().encode({ ...offer, tokenBWantedAmount: 600_000_000n }),
    });
    const transactionsBefore = cluster.transactions.length;

    await page.getByRole("button", { name: "Pay 540 tUSDC, get 3 tNVDAx" }).click();
    await page.getByText("The offer changed before your transaction landed").waitFor();

    assert.strictEqual(cluster.transactions.length, transactionsBefore, "no transaction landed");
    assert.strictEqual(await balanceOf(market.bob.address, MINTS.usdc.address), 1_250_000_000n);
    assert.strictEqual(await balanceOf(market.bob.address, MINTS.nvdax.address), null);
  });

  test("makes an offer and lands on it", async () => {
    await page.goto(`${app.url}/make`);
    await page.getByRole("combobox", { name: "Token" }).first().selectOption({ label: "tUSDC: you hold 1,250" });
    await page.locator("#amount-a").fill("100");
    await page.getByRole("combobox", { name: "Token" }).nth(1).selectOption({ label: "tNVDAx: Test NVDAx" });
    await page.locator("#amount-b").fill("0.5");
    await page.getByText("That prices 1 tUSDC at 0.005 tNVDAx").waitFor();
    await page.getByRole("button", { name: "Make offer" }).click();

    await page.getByText("This is your offer.").waitFor();
    const offerAddress = page.url().split("/offers/")[1] as Address;
    const account = cluster.accounts.get(offerAddress);
    assert.ok(account, "the offer account exists where the page says");
    const offer = getOfferDecoder().decode(account.data);
    assert.strictEqual(offer.maker, market.bob.address);
    assert.strictEqual(offer.tokenMintA, MINTS.usdc.address);
    assert.strictEqual(offer.tokenMintB, MINTS.nvdax.address);
    assert.strictEqual(offer.tokenBWantedAmount, 50_000_000n);
    assert.strictEqual(await balanceOf(offerAddress, MINTS.usdc.address), 100_000_000n, "the vault holds 100 tUSDC");
    assert.strictEqual(await balanceOf(market.bob.address, MINTS.usdc.address), 1_150_000_000n);
  });

  test("cancels that offer and gets the tokens back", async () => {
    const offerAddress = page.url().split("/offers/")[1] as Address;
    await page.getByRole("button", { name: "Cancel offer and take it all back" }).click();
    await page.getByRole("heading", { name: "Offer cancelled" }).waitFor();
    assert.strictEqual(cluster.accounts.has(offerAddress), false);
    assert.strictEqual(await balanceOf(market.bob.address, MINTS.usdc.address), 1_250_000_000n);
  });

  test("says when an offer is gone", async () => {
    await page.goto(`${app.url}/offers/${market.offers.alice}`);
    await page.getByRole("heading", { name: "This offer is gone" }).waitFor();
  });
});
