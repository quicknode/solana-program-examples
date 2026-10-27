import type { Address, TransactionSigner } from "@solana/kit";
import { findAssociatedTokenPda } from "@solana-program/token";
import {
  findOfferPda,
  getCancelOfferInstruction,
  getMakeOfferInstructionAsync,
  getTakeOfferInstruction,
} from "../generated";
import type { OfferListing } from "./offers";
import type { MintInfo } from "./tokens";

// Build the escrow program's three instructions. The generated client knows
// each instruction's layout; what it cannot know is which token accounts a
// particular wallet and offer use. Every one of them is an associated token
// account, so each is derived here from its owner, its mint, and the mint's
// token program.

const associatedTokenAccount = async (owner: Address, mint: MintInfo): Promise<Address> => {
  const [tokenAccount] = await findAssociatedTokenPda({ owner, mint: mint.address, tokenProgram: mint.tokenProgram });
  return tokenAccount;
};

// A random offer id. The id is one of the offer address's seeds, so a maker
// with several offers open needs a different id for each; a random u64 makes
// a collision with the maker's own earlier offers vanishingly unlikely.
export const generateOfferId = (): bigint => crypto.getRandomValues(new BigUint64Array(1))[0];

export type MakeOfferRequest = {
  maker: TransactionSigner;
  mintA: MintInfo;
  mintB: MintInfo;
  offeredAmount: bigint;
  wantedAmount: bigint;
  id: bigint;
  programAddress: Address;
};

export const buildMakeOfferInstruction = async (request: MakeOfferRequest) => {
  const { maker, mintA, mintB, id, programAddress } = request;
  // The program passes one token program for both mints, so both must belong
  // to it. The form refuses a mixed pair before it gets here.
  if (mintA.tokenProgram !== mintB.tokenProgram) {
    throw new Error("Both tokens must belong to the same token program");
  }
  const [offer] = await findOfferPda({ maker: maker.address, id }, { programAddress });
  return getMakeOfferInstructionAsync(
    {
      maker,
      tokenMintA: mintA.address,
      tokenMintB: mintB.address,
      makerTokenAccountA: await associatedTokenAccount(maker.address, mintA),
      makerTokenAccountB: await associatedTokenAccount(maker.address, mintB),
      offer,
      vault: await associatedTokenAccount(offer, mintA),
      tokenProgram: mintA.tokenProgram,
      id,
      tokenAOfferedAmount: request.offeredAmount,
      tokenBWantedAmount: request.wantedAmount,
    },
    { programAddress },
  );
};

// The taker's bounds are the offer as the taker saw it: at least the token A
// the vault held, for at most the token B the offer wanted. If the maker
// swaps the offer for a worse one before this lands, the program refuses it.
export const buildTakeOfferInstruction = async (
  taker: TransactionSigner,
  offer: OfferListing,
  programAddress: Address,
) =>
  getTakeOfferInstruction(
    {
      taker,
      maker: offer.maker,
      tokenMintA: offer.mintA.address,
      tokenMintB: offer.mintB.address,
      takerTokenAccountA: await associatedTokenAccount(taker.address, offer.mintA),
      takerTokenAccountB: await associatedTokenAccount(taker.address, offer.mintB),
      makerTokenAccountB: await associatedTokenAccount(offer.maker, offer.mintB),
      offer: offer.address,
      vault: offer.vault,
      tokenProgram: offer.mintA.tokenProgram,
      minimumTokenAOut: offer.offeredAmount,
      maximumTokenBIn: offer.wantedAmount,
    },
    { programAddress },
  );

export const buildCancelOfferInstruction = async (
  maker: TransactionSigner,
  offer: OfferListing,
  programAddress: Address,
) =>
  getCancelOfferInstruction(
    {
      maker,
      tokenMintA: offer.mintA.address,
      makerTokenAccountA: await associatedTokenAccount(maker.address, offer.mintA),
      offer: offer.address,
      vault: offer.vault,
      tokenProgram: offer.mintA.tokenProgram,
    },
    { programAddress },
  );
