import assert from "node:assert";
import { createHash } from "node:crypto";
import { readFileSync } from "node:fs";
import { describe, test } from "node:test";
import {
  AccountRole,
  createNoopSigner,
  getAddressEncoder,
  getProgramDerivedAddress,
  getU64Encoder,
  getUtf8Encoder,
} from "@solana/kit";
import { TOKEN_PROGRAM_ADDRESS } from "@solana-program/token";
import {
  CANCEL_OFFER_DISCRIMINATOR,
  ESCROW_ERROR__OFFER_TERMS_CHANGED,
  ESCROW_ERROR__ZERO_AMOUNT,
  ESCROW_PROGRAM_ADDRESS,
  getMakeOfferInstructionAsync,
  getOfferDecoder,
  getOfferEncoder,
  getOfferSize,
  getTakeOfferInstruction,
  MAKE_OFFER_DISCRIMINATOR,
  OFFER_DISCRIMINATOR,
  TAKE_OFFER_DISCRIMINATOR,
} from "../src/generated";
import { testAddress } from "./fake-rpc";

// These tests check the generated client against the program itself, not
// against the IDL it was generated from: Anchor dispatches on the first eight
// bytes of sha256("global:<handler name>") and marks accounts with the first
// eight of sha256("account:<struct name>"), and lays out arguments and fields
// in Borsh, little-endian, in declaration order.

const anchorDiscriminator = (preimage: string) => [...createHash("sha256").update(preimage).digest().subarray(0, 8)];

const idl = JSON.parse(readFileSync(new URL("../idl/escrow.json", import.meta.url), "utf8"));

const u64 = (value: bigint) => [...getU64Encoder().encode(value)];

// Stand-in addresses: 32 bytes of one repeated value each.
const ALICE = testAddress(1);
const BOB = testAddress(2);
const USDC_MINT = testAddress(3);
const TSLAX_MINT = testAddress(4);
const ALICE_USDC = testAddress(5);
const ALICE_TSLAX = testAddress(6);
const VAULT = testAddress(7);

describe("discriminators", () => {
  test("instruction discriminators are sha256 of global:<handler>", () => {
    assert.deepStrictEqual([...MAKE_OFFER_DISCRIMINATOR], anchorDiscriminator("global:make_offer"));
    assert.deepStrictEqual([...TAKE_OFFER_DISCRIMINATOR], anchorDiscriminator("global:take_offer"));
    assert.deepStrictEqual([...CANCEL_OFFER_DISCRIMINATOR], anchorDiscriminator("global:cancel_offer"));
  });

  test("the offer account discriminator is sha256 of account:Offer", () => {
    assert.deepStrictEqual([...OFFER_DISCRIMINATOR], anchorDiscriminator("account:Offer"));
  });

  test("the program address is the one in declare_id!", () => {
    const source = readFileSync(new URL("../../anchor/programs/escrow/src/lib.rs", import.meta.url), "utf8");
    assert.match(source, new RegExp(`declare_id!\\("${ESCROW_PROGRAM_ADDRESS}"\\)`));
  });

  test("error codes start at Anchor's 6000, in the order of the program's EscrowError", () => {
    assert.strictEqual(ESCROW_ERROR__ZERO_AMOUNT, 6000);
    assert.strictEqual(ESCROW_ERROR__OFFER_TERMS_CHANGED, 6001);
  });
});

describe("offer account", () => {
  test("is 8 discriminator bytes plus Offer::INIT_SPACE", () => {
    // id (8) + maker (32) + two mints (64) + token_b_wanted_amount (8) + bump (1)
    assert.strictEqual(getOfferSize(), 8 + 8 + 32 + 64 + 8 + 1);
  });

  test("decodes the bytes the program writes", () => {
    const bytes = new Uint8Array([
      ...anchorDiscriminator("account:Offer"),
      ...u64(7n),
      ...getAddressEncoder().encode(ALICE),
      ...getAddressEncoder().encode(USDC_MINT),
      ...getAddressEncoder().encode(TSLAX_MINT),
      ...u64(100_000_000n),
      254,
    ]);
    const offer = getOfferDecoder().decode(bytes);
    assert.strictEqual(offer.id, 7n);
    assert.strictEqual(offer.maker, ALICE);
    assert.strictEqual(offer.tokenMintA, USDC_MINT);
    assert.strictEqual(offer.tokenMintB, TSLAX_MINT);
    assert.strictEqual(offer.tokenBWantedAmount, 100_000_000n);
    assert.strictEqual(offer.bump, 254);
    assert.deepStrictEqual(
      [...getOfferEncoder().encode({ ...offer })],
      [...bytes],
      "encoding the decoded offer gives back the same bytes",
    );
  });
});

describe("make_offer", () => {
  test("encodes id, token_a_offered_amount and token_b_wanted_amount, and derives the offer PDA", async () => {
    const instruction = await getMakeOfferInstructionAsync({
      maker: createNoopSigner(ALICE),
      tokenMintA: USDC_MINT,
      tokenMintB: TSLAX_MINT,
      makerTokenAccountA: ALICE_USDC,
      makerTokenAccountB: ALICE_TSLAX,
      vault: VAULT,
      id: 7n,
      tokenAOfferedAmount: 250_000_000n,
      tokenBWantedAmount: 100_000_000n,
    });

    assert.deepStrictEqual(
      [...instruction.data],
      [...anchorDiscriminator("global:make_offer"), ...u64(7n), ...u64(250_000_000n), ...u64(100_000_000n)],
    );

    const [expectedOffer] = await getProgramDerivedAddress({
      programAddress: ESCROW_PROGRAM_ADDRESS,
      seeds: [getUtf8Encoder().encode("offer"), getAddressEncoder().encode(ALICE), getU64Encoder().encode(7n)],
    });

    const idlAccounts = idl.instructions.find((handler: { name: string }) => handler.name === "make_offer").accounts;
    assert.strictEqual(instruction.accounts.length, idlAccounts.length);
    const byName = Object.fromEntries(
      idlAccounts.map((account: { name: string }, index: number) => [account.name, instruction.accounts[index]]),
    );
    assert.strictEqual(byName.maker.address, ALICE);
    assert.strictEqual(byName.maker.role, AccountRole.WRITABLE_SIGNER);
    assert.strictEqual(byName.offer.address, expectedOffer);
    assert.strictEqual(byName.vault.address, VAULT);
    assert.strictEqual(byName.associated_token_program.address, "ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL");
    assert.strictEqual(byName.token_program.address, TOKEN_PROGRAM_ADDRESS);
    assert.strictEqual(byName.system_program.address, "11111111111111111111111111111111");
  });
});

describe("take_offer", () => {
  test("encodes the taker's bounds and marks only the taker as a signer", () => {
    const offerAddress = testAddress(8);
    const instruction = getTakeOfferInstruction({
      taker: createNoopSigner(BOB),
      maker: ALICE,
      tokenMintA: USDC_MINT,
      tokenMintB: TSLAX_MINT,
      takerTokenAccountA: testAddress(9),
      takerTokenAccountB: testAddress(10),
      makerTokenAccountB: ALICE_TSLAX,
      offer: offerAddress,
      vault: VAULT,
      minimumTokenAOut: 250_000_000n,
      maximumTokenBIn: 100_000_000n,
    });

    assert.deepStrictEqual(
      [...instruction.data],
      [...anchorDiscriminator("global:take_offer"), ...u64(250_000_000n), ...u64(100_000_000n)],
    );
    const signers = instruction.accounts.filter(
      (account) => account.role === AccountRole.WRITABLE_SIGNER || account.role === AccountRole.READONLY_SIGNER,
    );
    assert.deepStrictEqual(
      signers.map((account) => account.address),
      [BOB],
    );
    const idlAccounts = idl.instructions.find((handler: { name: string }) => handler.name === "take_offer").accounts;
    assert.deepStrictEqual(
      instruction.accounts.map((account) => account.address).slice(0, 2),
      [BOB, ALICE],
      `accounts follow the IDL order: ${idlAccounts.map((account: { name: string }) => account.name).join(", ")}`,
    );
    assert.strictEqual(instruction.accounts[7].address, offerAddress);
  });
});
