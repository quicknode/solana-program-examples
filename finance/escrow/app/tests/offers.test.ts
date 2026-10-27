import assert from "node:assert";
import { describe, test } from "node:test";
import type { Address } from "@solana/kit";
import { findAssociatedTokenPda, TOKEN_PROGRAM_ADDRESS } from "@solana-program/token";
import { TOKEN_2022_PROGRAM_ADDRESS } from "@solana-program/token-2022";
import { ESCROW_PROGRAM_ADDRESS, findOfferPda, getOfferEncoder } from "../src/generated";
import type { OfferReadingRpc } from "../src/lib/offers";
import { fetchAllOffers, fetchOffer, sortOffers } from "../src/lib/offers";
import { createFakeRpc, type FakeAccount, testAddress } from "./fake-rpc";
import { mintAccount, tokenAccount } from "./fixtures";

// The tests' story is the book's: Alice offers 250 USDC for 1 TSLAx. USDC
// has 6 decimals and TSLAx 8. Carol, another maker, offers 500 USDC for
// 2.5 TSLAx, a better price for a taker.
const ALICE = testAddress(1);
const CAROL = testAddress(2);
const USDC_MINT = testAddress(3);
const TSLAX_MINT = testAddress(4);
const SOME_OTHER_PROGRAM = testAddress(5);

const addOffer = async (
  accounts: Map<Address, FakeAccount>,
  maker: Address,
  id: bigint,
  vaultAmount: bigint | null,
  wanted: bigint,
  tokenProgram: Address,
) => {
  const [offerAddress, bump] = await findOfferPda({ maker, id });
  accounts.set(offerAddress, {
    owner: ESCROW_PROGRAM_ADDRESS,
    data: getOfferEncoder().encode({
      id,
      maker,
      tokenMintA: USDC_MINT,
      tokenMintB: TSLAX_MINT,
      tokenBWantedAmount: wanted,
      bump,
    }),
  });
  const [vault] = await findAssociatedTokenPda({ owner: offerAddress, mint: USDC_MINT, tokenProgram });
  if (vaultAmount !== null) {
    accounts.set(vault, tokenAccount(USDC_MINT, offerAddress, vaultAmount, tokenProgram));
  }
  return { offerAddress, vault };
};

const buildMarket = async (tokenProgram: Address = TOKEN_PROGRAM_ADDRESS) => {
  const accounts = new Map<Address, FakeAccount>();
  accounts.set(USDC_MINT, mintAccount(6, tokenProgram, { name: "USD Coin", symbol: "USDC" }));
  accounts.set(TSLAX_MINT, mintAccount(8, tokenProgram, { name: "Tesla xStock", symbol: "TSLAx" }));
  const alice = await addOffer(accounts, ALICE, 1n, 250_000_000n, 100_000_000n, tokenProgram);
  const carol = await addOffer(accounts, CAROL, 1n, 500_000_000n, 250_000_000n, tokenProgram);
  return { accounts, alice, carol, rpc: createFakeRpc(accounts) as unknown as OfferReadingRpc };
};

describe("fetchAllOffers", () => {
  test("reads each offer's amounts, names and prices", async () => {
    const { rpc, alice } = await buildMarket(TOKEN_2022_PROGRAM_ADDRESS);
    const offers = await fetchAllOffers(rpc, ESCROW_PROGRAM_ADDRESS);
    assert.strictEqual(offers.length, 2);

    const aliceOffer = offers.find((offer) => offer.maker === ALICE);
    assert.ok(aliceOffer);
    assert.strictEqual(aliceOffer.address, alice.offerAddress);
    assert.strictEqual(aliceOffer.vault, alice.vault);
    assert.strictEqual(aliceOffer.offeredAmount, 250_000_000n);
    assert.strictEqual(aliceOffer.wantedAmount, 100_000_000n);
    assert.strictEqual(aliceOffer.mintA.symbol, "USDC");
    assert.strictEqual(aliceOffer.mintA.decimals, 6);
    assert.strictEqual(aliceOffer.mintA.tokenProgram, TOKEN_2022_PROGRAM_ADDRESS);
    assert.strictEqual(aliceOffer.mintB.symbol, "TSLAx");
    // 1 TSLAx for 250 USDC: a USDC costs 0.004 TSLAx, a TSLAx 250 USDC.
    assert.strictEqual(aliceOffer.priceOfAInB, 0.004);
    assert.strictEqual(aliceOffer.priceOfBInA, 250);
  });

  test("takes the amount on offer from the vault's balance, not from the offer account", async () => {
    const { accounts, alice } = await buildMarket();
    // Someone sends Alice's vault 1 USDC more. take_offer pays out the whole
    // vault, so that is what a taker would receive.
    accounts.set(alice.vault, tokenAccount(USDC_MINT, alice.offerAddress, 251_000_000n, TOKEN_PROGRAM_ADDRESS));
    const offer = await fetchOffer(
      createFakeRpc(accounts) as unknown as OfferReadingRpc,
      ESCROW_PROGRAM_ADDRESS,
      alice.offerAddress,
    );
    assert.strictEqual(offer?.offeredAmount, 251_000_000n);
  });

  test("leaves out an offer whose vault is missing", async () => {
    const { accounts } = await buildMarket();
    await addOffer(accounts, ALICE, 2n, null, 1n, TOKEN_PROGRAM_ADDRESS);
    const offers = await fetchAllOffers(createFakeRpc(accounts) as unknown as OfferReadingRpc, ESCROW_PROGRAM_ADDRESS);
    assert.strictEqual(offers.length, 2);
  });

  test("names an unnamed classic mint by its shortened address", async () => {
    const { accounts } = await buildMarket();
    accounts.set(TSLAX_MINT, mintAccount(8, TOKEN_PROGRAM_ADDRESS));
    const [offer] = await fetchAllOffers(createFakeRpc(accounts) as unknown as OfferReadingRpc, ESCROW_PROGRAM_ADDRESS);
    assert.strictEqual(offer.mintB.isNamed, false);
    assert.strictEqual(offer.mintB.symbol, `${TSLAX_MINT.slice(0, 4)}…${TSLAX_MINT.slice(-4)}`);
  });
});

describe("fetchOffer", () => {
  test("returns null where no offer lives", async () => {
    const { rpc } = await buildMarket();
    assert.strictEqual(await fetchOffer(rpc, ESCROW_PROGRAM_ADDRESS, testAddress(42)), null);
  });

  test("returns null for an account another program owns", async () => {
    const { accounts, alice } = await buildMarket();
    const offerAccount = accounts.get(alice.offerAddress);
    assert.ok(offerAccount);
    accounts.set(alice.offerAddress, { ...offerAccount, owner: SOME_OTHER_PROGRAM });
    const rpc = createFakeRpc(accounts) as unknown as OfferReadingRpc;
    assert.strictEqual(await fetchOffer(rpc, ESCROW_PROGRAM_ADDRESS, alice.offerAddress), null);
  });
});

describe("sortOffers", () => {
  test("puts the cheapest token A first, or the largest or smallest offer", async () => {
    const { rpc } = await buildMarket();
    const offers = await fetchAllOffers(rpc, ESCROW_PROGRAM_ADDRESS);
    // Carol's USDC costs 0.005 TSLAx each, Alice's 0.004.
    assert.deepStrictEqual(
      sortOffers(offers, "best-price").map((offer) => offer.maker),
      [ALICE, CAROL],
    );
    assert.deepStrictEqual(
      sortOffers(offers, "largest").map((offer) => offer.maker),
      [CAROL, ALICE],
    );
    assert.deepStrictEqual(
      sortOffers(offers, "smallest").map((offer) => offer.maker),
      [ALICE, CAROL],
    );
  });
});
