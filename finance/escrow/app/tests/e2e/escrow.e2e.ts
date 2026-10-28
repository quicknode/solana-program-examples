import assert from "node:assert";
import { spawn } from "node:child_process";
import { mkdirSync } from "node:fs";
import { after, before, describe, test as nodeTest } from "node:test";
import type { Address } from "@solana/kit";
import { findAssociatedTokenPda, TOKEN_PROGRAM_ADDRESS } from "@solana-program/token";
import { getTokenDecoder, TOKEN_2022_PROGRAM_ADDRESS } from "@solana-program/token-2022";
import { type Browser, chromium, type Page } from "playwright";
import {
  ESCROW_PROGRAM_ADDRESS,
  getOfferDecoder,
  getOfferEncoder,
  parseTakeOfferInstruction,
} from "../../src/generated";
import { buildAndStartApp } from "./app-server";
import { buildMarket, MAKERS, type Market, MINTS, PRESET_MINTS } from "./market";
import { type MockCluster, startMockCluster } from "./mock-cluster";
import { mockWalletScript } from "./mock-wallet";

// The app, built for devnet and served against a mock of it, driven in
// Chromium by a test wallet that approves everything. The viewer is Bob; see
// market.ts for what everyone holds. These tests run in order: each starts
// from the market the one before it left.

const CLUSTER_PORT = 18899;
const APP_PORT = 3100;

let market: Market;
let cluster: MockCluster;
let app: { url: string; stop: () => void };
let browser: Browser;
let page: Page;

const ONE_SOL = 1_000_000_000n;

const balanceOf = async (owner: Address, mint: Address): Promise<bigint | null> => {
  const tokenProgram =
    cluster.accounts.get(mint)?.owner === TOKEN_PROGRAM_ADDRESS ? TOKEN_PROGRAM_ADDRESS : TOKEN_2022_PROGRAM_ADDRESS;
  const [tokenAccount] = await findAssociatedTokenPda({ owner, mint, tokenProgram });
  const account = cluster.accounts.get(tokenAccount);
  return account ? getTokenDecoder().decode(account.data).amount : null;
};

const lamportsOf = (owner: Address): bigint => cluster.accounts.get(owner)?.lamports ?? 0n;

const offerCards = () => page.locator(".offer-card");

// The make form's two halves: what you give (a select), what you want (chips).
const giveSelect = () => page.getByRole("group", { name: "1. What you give" }).getByRole("combobox");
const wantChip = (name: string) =>
  page.getByRole("group", { name: "2. What you want" }).getByRole("radio", { name, exact: true });
// Each radio is visually hidden inside its chip, so pick a token the way a
// person does: by clicking the chip.
const chooseWant = (name: string) => wantChip(name).locator("xpath=..").click();

const makeOffer = async (give: Address, giveAmount: string, want: string, wantAmount: string) => {
  await page.goto(`${app.url}/make`);
  await giveSelect().selectOption(give);
  await page.locator("#amount-a").fill(giveAmount);
  await chooseWant(want);
  await page.locator("#amount-b").fill(wantAmount);
  await page.getByRole("button", { name: "Make offer" }).click();
  await page.getByText("This is your offer.").waitFor();
  return page.url().split("/offers/")[1] as Address;
};

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
  await page.addInitScript(mockWalletScript(market.bobPrivateKey, market.bob.address, "solana:devnet"));
});

after(async () => {
  await browser?.close();
  app?.stop();
  await cluster?.close();
});

describe("the escrow app", () => {
  test("lists every open offer before a wallet connects", async () => {
    await page.goto(app.url);
    await page.getByText("6 of 6 open offers").waitFor();
    assert.strictEqual(await offerCards().count(), 6);
    assert.strictEqual(await page.locator(".status-tag").count(), 0, "no affordability without a wallet");
  });

  test("connects the wallet, shows Bob's .sol name, and says which offers he can pay for", async () => {
    await page.getByRole("button", { name: "Connect wallet" }).click();
    await page.getByRole("button", { name: "Test Wallet" }).click();
    await page.getByText("Your offer").waitFor();
    // Bob's primary SNS name, in place of his address.
    await page.getByRole("button", { name: "bob.sol" }).waitFor();

    const aliceCard = offerCards().filter({ hasText: "250 tUSDC" });
    await aliceCard.getByText("You can take this").waitFor();
    // Carol wants 2.5 tTSLAx and Bob holds 1.5.
    await offerCards().filter({ hasText: "500 tUSDC" }).getByText("Need 1 more tTSLAx").waitFor();
  });

  test("filters to one pair, cheapest first", async () => {
    await page.getByLabel("You want to get").selectOption({ label: "tUSDC" });
    await page.getByLabel("You want to pay with").selectOption({ label: "tTSLAx" });
    await page.getByText("2 of 6 open offers").waitFor();
    const first = await offerCards().nth(0).textContent();
    assert.match(first ?? "", /250 tUSDC/, "Alice's 250 tUSDC a tTSLAx beats Carol's 200");

    await page.getByLabel("Only offers I can pay for").check();
    await page.getByText("1 of 6 open offers").waitFor();
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
    await giveSelect().selectOption(MINTS.usdc.address);
    await page.locator("#amount-a").fill("100");
    await chooseWant("tNVDAx");
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

  test("offers the cluster's popular tokens that are mints here, and disables the ones that can't pair", async () => {
    await page.goto(`${app.url}/make`);
    await giveSelect().selectOption(MINTS.usdc.address);
    // The mock devnet holds SOL, USDC and PYUSD at their devnet addresses,
    // and none of the other presets: those must not be offered.
    for (const shown of ["SOL", "USDC", "PYUSD"]) {
      await wantChip(shown).waitFor();
    }
    for (const hidden of ["EURC", "USDG", "PAXG", "JitoSOL"]) {
      assert.strictEqual(await wantChip(hidden).count(), 0, `${hidden} is not a mint here`);
    }
    // tUSDC is a Token-2022 token, so the classic token program's SOL and
    // USDC cannot be paired with it; PYUSD, also Token-2022, can.
    assert.strictEqual(await wantChip("SOL").isDisabled(), true);
    assert.strictEqual(await wantChip("USDC").isDisabled(), true);
    assert.strictEqual(await wantChip("PYUSD").isDisabled(), false);
    assert.strictEqual(await wantChip("tUSDC").isDisabled(), true, "the token being given");
    await page.getByText("SOL, USDC: made with the classic token program, and tUSDC with Token-2022.").waitFor();
  });

  test("Other takes any mint address and checks it", async () => {
    await page.goto(`${app.url}/make`);
    await giveSelect().selectOption(MINTS.usdc.address);
    await chooseWant("Other…");
    await page.getByLabel("Mint address").fill("not an address");
    await page.getByText("That is not a Solana address.").waitFor();
    await page.getByLabel("Mint address").fill(MAKERS.alice);
    await page.getByText("No token mint lives at that address on this cluster.").waitFor();
    await page.getByLabel("Mint address").fill(MINTS.nvdax.address);
    await page.getByText("Test NVDAx (tNVDAx), 8 decimal places.").waitFor();
  });

  test("takes an offer of SOL: Bob pays 300 USDC and gets 2 SOL as plain SOL", async () => {
    const lamportsBefore = lamportsOf(market.bob.address);
    await page.goto(`${app.url}/offers/${market.offers.erin}`);
    await page.getByRole("button", { name: "Pay 300 USDC, get 2 SOL" }).click();
    await page.getByRole("heading", { name: "Offer taken" }).waitFor();

    // The wrapped SOL was unwrapped in the same transaction. Bob's earlier
    // 0.25 wrapped SOL came out with it, and the token account's rent too.
    assert.strictEqual(await balanceOf(market.bob.address, PRESET_MINTS.sol), null, "no SOL token account left");
    assert.strictEqual(lamportsOf(market.bob.address) - lamportsBefore, 2n * ONE_SOL + ONE_SOL / 4n + 2_039_280n);
    assert.strictEqual(await balanceOf(market.bob.address, PRESET_MINTS.usdc), 200_000_000n);
    assert.strictEqual(await balanceOf(MAKERS.erin, PRESET_MINTS.usdc), 300_000_000n);
  });

  test("takes an offer wanting SOL: Bob pays 0.5 SOL from his wallet's SOL", async () => {
    const lamportsBefore = lamportsOf(market.bob.address);
    await page.goto(`${app.url}/offers/${market.offers.frank}`);
    await page.getByRole("button", { name: "Pay 0.5 SOL, get 100 USDC" }).click();
    await page.getByRole("heading", { name: "Offer taken" }).waitFor();

    assert.strictEqual(lamportsBefore - lamportsOf(market.bob.address), ONE_SOL / 2n, "exactly 0.5 SOL left Bob");
    assert.strictEqual(await balanceOf(market.bob.address, PRESET_MINTS.sol), null, "the wrapping account is closed");
    assert.strictEqual(await balanceOf(MAKERS.frank, PRESET_MINTS.sol), ONE_SOL / 2n, "Frank is paid wrapped SOL");
    assert.strictEqual(await balanceOf(market.bob.address, PRESET_MINTS.usdc), 300_000_000n);
  });

  test("offers SOL, then cancels and gets it back as plain SOL", async () => {
    const lamportsBefore = lamportsOf(market.bob.address);
    const offerAddress = await makeOffer(PRESET_MINTS.sol, "0.4", "USDC", "60");
    assert.strictEqual(
      await balanceOf(offerAddress, PRESET_MINTS.sol),
      (ONE_SOL * 4n) / 10n,
      "the vault holds 0.4 SOL",
    );

    await page.getByRole("button", { name: "Cancel offer and take it all back" }).click();
    await page.getByRole("heading", { name: "Offer cancelled" }).waitFor();
    assert.strictEqual(cluster.accounts.has(offerAddress), false);
    assert.strictEqual(await balanceOf(market.bob.address, PRESET_MINTS.sol), null);
    assert.strictEqual(
      lamportsOf(market.bob.address),
      lamportsBefore,
      "every lamport and every rent deposit came back",
    );
  });

  test("asks for SOL, is paid wrapped SOL, and unwraps it without closing the account the offer needs", async () => {
    await makeOffer(PRESET_MINTS.usdc, "50", "SOL", "0.3");
    // make_offer opened Bob's SOL token account for the taker to pay into.
    // Stand in for a taker's payment by crediting it 0.3 wrapped SOL.
    const [wrappedSolAccount] = await findAssociatedTokenPda({
      owner: market.bob.address,
      mint: PRESET_MINTS.sol,
      tokenProgram: TOKEN_PROGRAM_ADDRESS,
    });
    const stored = cluster.accounts.get(wrappedSolAccount);
    assert.ok(stored, "make_offer created the account");
    const payment = (ONE_SOL * 3n) / 10n;
    const { tokenAccount } = await import("../fixtures");
    cluster.accounts.set(wrappedSolAccount, {
      ...tokenAccount(PRESET_MINTS.sol, market.bob.address, payment, TOKEN_PROGRAM_ADDRESS),
      lamports: stored.lamports + payment,
    });
    const lamportsBefore = lamportsOf(market.bob.address);

    await page.goto(`${app.url}/make`);
    await page.getByText("You hold 0.3 wrapped SOL").waitFor();
    await page.getByText("You still have offers asking for SOL").waitFor();
    await page.getByRole("button", { name: "Unwrap SOL" }).click();
    await page.getByText("You hold 0.3 wrapped SOL").waitFor({ state: "detached" });

    assert.strictEqual(
      lamportsOf(market.bob.address) - lamportsBefore,
      payment,
      "the SOL is back; the rent re-funds the new account",
    );
    assert.strictEqual(await balanceOf(market.bob.address, PRESET_MINTS.sol), 0n, "an empty SOL account stays open");
  });

  test("check-popular-tokens reports which presets are mints on the cluster", async () => {
    // Asynchronously: the mock cluster answering the script runs in this
    // process, so a blocking spawn would never get its answers.
    const result = await new Promise<{ stdout: string; status: number | null }>((resolve) => {
      const child = spawn(
        "node_modules/.bin/tsx",
        ["scripts/check-popular-tokens.ts", "--cluster", "devnet", "--rpc", cluster.rpcUrl],
        { cwd: new URL("../..", import.meta.url).pathname },
      );
      let stdout = "";
      child.stdout.on("data", (chunk) => {
        stdout += chunk;
      });
      child.on("exit", (status) => resolve({ stdout, status }));
    });
    assert.match(result.stdout, /^ok {4}SOL /m);
    assert.match(result.stdout, /^ok {4}USDC /m);
    assert.match(result.stdout, /^FAIL {2}EURC /m);
    assert.match(result.stdout, /3 of 7 popular tokens are mints on devnet/);
    assert.strictEqual(result.status, 1, "a missing mint fails the check");
  });

  test("says when an offer is gone", async () => {
    await page.goto(`${app.url}/offers/${market.offers.alice}`);
    await page.getByRole("heading", { name: "This offer is gone" }).waitFor();
  });
});
