import assert from "node:assert";
import { describe, test } from "node:test";
import { address, createNoopSigner, getU64Decoder, type Instruction } from "@solana/kit";
import { SYSTEM_PROGRAM_ADDRESS } from "@solana-program/system";
import { findAssociatedTokenPda, TOKEN_PROGRAM_ADDRESS } from "@solana-program/token";
import { NATIVE_MINT_ADDRESS } from "../src/components/token-picker/popular-token-addresses";
import { ESCROW_PROGRAM_ADDRESS, identifyEscrowInstruction } from "../src/generated";
import {
  buildCancelOfferInstructions,
  buildMakeOfferInstructions,
  buildTakeOfferInstructions,
  buildUnwrapSolInstructions,
} from "../src/lib/escrow-instructions";
import type { OfferListing } from "../src/lib/offers";
import type { MintInfo } from "../src/lib/tokens";
import { testAddress } from "./fake-rpc";

// The instructions around each escrow instruction when SOL is one side of
// the trade: wrap before paying SOL, unwrap after receiving it, and nothing
// extra when neither side is SOL.

const ASSOCIATED_TOKEN_PROGRAM = "ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL";
const ALICE = createNoopSigner(testAddress(1));
const BOB = createNoopSigner(testAddress(2));

const mint = (mintAddress: string, symbol: string, decimals: number): MintInfo => ({
  address: address(mintAddress),
  tokenProgram: TOKEN_PROGRAM_ADDRESS,
  decimals,
  mintAuthority: null,
  symbol,
  name: symbol,
  isNamed: true,
});

const SOL = mint(NATIVE_MINT_ADDRESS, "SOL", 9);
const USDC = mint(testAddress(3), "USDC", 6);
const EURC = mint(testAddress(4), "EURC", 6);

const listing = (mintA: MintInfo, offered: bigint, mintB: MintInfo, wanted: bigint): OfferListing => ({
  address: testAddress(5),
  id: 1n,
  maker: ALICE.address,
  mintA,
  mintB,
  vault: testAddress(6),
  offeredAmount: offered,
  wantedAmount: wanted,
  priceOfAInB: 0,
  priceOfBInA: 0,
});

const wrappedSolAccount = async (owner: string) =>
  (
    await findAssociatedTokenPda({
      owner: address(owner),
      mint: address(NATIVE_MINT_ADDRESS),
      tokenProgram: TOKEN_PROGRAM_ADDRESS,
    })
  )[0];

// A readable name for each instruction: which program, and for the token
// program, which of its instructions (by its one-byte discriminator).
const TOKEN_INSTRUCTION_NAMES: Record<number, string> = { 9: "closeAccount", 17: "syncNative" };
const nameOf = (instruction: Instruction): string => {
  switch (instruction.programAddress) {
    case SYSTEM_PROGRAM_ADDRESS:
      return "system.transfer";
    case ASSOCIATED_TOKEN_PROGRAM:
      return "ata.createIdempotent";
    case TOKEN_PROGRAM_ADDRESS:
      return `token.${TOKEN_INSTRUCTION_NAMES[instruction.data?.[0] ?? -1] ?? "unknown"}`;
    case ESCROW_PROGRAM_ADDRESS:
      return `escrow.${["makeOffer", "takeOffer", "cancelOffer"][identifyEscrowInstruction(instruction as never)]}`;
    default:
      return instruction.programAddress;
  }
};

// The lamports a System Program transfer moves: a u64 after its 4-byte
// instruction index.
const transferAmount = (instruction: Instruction) => getU64Decoder().decode(instruction.data ?? new Uint8Array(), 4);

describe("SOL around the escrow instructions", () => {
  test("an offer between two tokens is the escrow instruction alone", async () => {
    const make = await buildMakeOfferInstructions({
      maker: ALICE,
      mintA: USDC,
      mintB: EURC,
      offeredAmount: 1n,
      wantedAmount: 1n,
      id: 1n,
      programAddress: ESCROW_PROGRAM_ADDRESS,
      wrappedSolBalance: 0n,
    });
    assert.deepStrictEqual(make.map(nameOf), ["escrow.makeOffer"]);
    const take = await buildTakeOfferInstructions(BOB, listing(USDC, 1n, EURC, 1n), ESCROW_PROGRAM_ADDRESS, 0n);
    assert.deepStrictEqual(take.map(nameOf), ["escrow.takeOffer"]);
  });

  test("offering SOL wraps only what the wallet's SOL token account lacks, before make_offer", async () => {
    const instructions = await buildMakeOfferInstructions({
      maker: ALICE,
      mintA: SOL,
      mintB: USDC,
      offeredAmount: 2_000_000_000n,
      wantedAmount: 300_000_000n,
      id: 1n,
      programAddress: ESCROW_PROGRAM_ADDRESS,
      wrappedSolBalance: 500_000_000n,
    });
    assert.deepStrictEqual(instructions.map(nameOf), [
      "ata.createIdempotent",
      "system.transfer",
      "token.syncNative",
      "escrow.makeOffer",
    ]);
    const wrapped = await wrappedSolAccount(ALICE.address);
    assert.strictEqual(transferAmount(instructions[1]), 1_500_000_000n);
    assert.strictEqual(instructions[1].accounts?.[1].address, wrapped);
    assert.strictEqual(instructions[2].accounts?.[0].address, wrapped);
  });

  test("offering SOL already wrapped in full moves no lamports", async () => {
    const instructions = await buildMakeOfferInstructions({
      maker: ALICE,
      mintA: SOL,
      mintB: USDC,
      offeredAmount: 1_000_000_000n,
      wantedAmount: 1n,
      id: 1n,
      programAddress: ESCROW_PROGRAM_ADDRESS,
      wrappedSolBalance: 1_000_000_000n,
    });
    assert.deepStrictEqual(instructions.map(nameOf), ["ata.createIdempotent", "escrow.makeOffer"]);
  });

  test("paying SOL for an offer wraps it before take_offer and closes the emptied account after", async () => {
    const instructions = await buildTakeOfferInstructions(
      BOB,
      listing(USDC, 250_000_000n, SOL, 1_500_000_000n),
      ESCROW_PROGRAM_ADDRESS,
      0n,
    );
    assert.deepStrictEqual(instructions.map(nameOf), [
      "ata.createIdempotent",
      "system.transfer",
      "token.syncNative",
      "escrow.takeOffer",
      "token.closeAccount",
    ]);
    assert.strictEqual(transferAmount(instructions[1]), 1_500_000_000n);
  });

  test("receiving SOL from an offer unwraps it after take_offer, to the taker", async () => {
    const instructions = await buildTakeOfferInstructions(
      BOB,
      listing(SOL, 1_000_000_000n, USDC, 150_000_000n),
      ESCROW_PROGRAM_ADDRESS,
      0n,
    );
    assert.deepStrictEqual(instructions.map(nameOf), ["escrow.takeOffer", "token.closeAccount"]);
    const close = instructions[1];
    assert.strictEqual(close.accounts?.[0].address, await wrappedSolAccount(BOB.address));
    assert.strictEqual(close.accounts?.[1].address, BOB.address, "the SOL goes to the taker");
  });

  test("cancelling an offer of SOL makes sure the maker's SOL account exists, then unwraps it", async () => {
    const instructions = await buildCancelOfferInstructions(
      ALICE,
      listing(SOL, 1_000_000_000n, USDC, 150_000_000n),
      ESCROW_PROGRAM_ADDRESS,
    );
    assert.deepStrictEqual(instructions.map(nameOf), [
      "ata.createIdempotent",
      "escrow.cancelOffer",
      "token.closeAccount",
    ]);
    assert.strictEqual(instructions[2].accounts?.[1].address, ALICE.address);
  });

  test("unwrapping for a maker with offers asking for SOL keeps an empty SOL account open", async () => {
    assert.deepStrictEqual((await buildUnwrapSolInstructions(ALICE)).map(nameOf), ["token.closeAccount"]);
    assert.deepStrictEqual((await buildUnwrapSolInstructions(ALICE, true)).map(nameOf), [
      "token.closeAccount",
      "ata.createIdempotent",
    ]);
  });

  test("cancelling an offer of another token is cancel_offer alone", async () => {
    const instructions = await buildCancelOfferInstructions(ALICE, listing(USDC, 1n, SOL, 1n), ESCROW_PROGRAM_ADDRESS);
    assert.deepStrictEqual(instructions.map(nameOf), ["escrow.cancelOffer"]);
  });
});
