import { type Address, address, type Instruction, type TransactionSigner } from "@solana/kit";
import { getTransferSolInstruction } from "@solana-program/system";
import {
  findAssociatedTokenPda,
  getCloseAccountInstruction,
  getCreateAssociatedTokenIdempotentInstructionAsync,
  getSyncNativeInstruction,
  TOKEN_PROGRAM_ADDRESS,
} from "@solana-program/token";
import { NATIVE_MINT_ADDRESS } from "@/components/token-picker/popular-token-addresses";
import {
  findOfferPda,
  getCancelOfferInstruction,
  getMakeOfferInstructionAsync,
  getTakeOfferInstruction,
} from "../generated";
import type { OfferListing } from "./offers";
import { isNativeSol, type MintInfo } from "./tokens";

// Build the transactions for the escrow program's three instructions. The
// generated client knows each instruction's layout; what it cannot know is
// which token accounts a particular wallet and offer use. Every one of them is
// an associated token account, so each is derived here from its owner, its
// mint, and the mint's token program.
//
// SOL is handled for the user. The escrow program moves tokens, and SOL as a
// token is the native mint: lamports held in a token account and counted by
// it. So where a wallet pays SOL, the transaction first moves the lamports
// into the wallet's native-mint account and syncs its balance ("wrapping"),
// and where a wallet receives SOL, the transaction closes that account
// afterwards, which turns its whole balance back into SOL ("unwrapping").

const NATIVE_MINT = address(NATIVE_MINT_ADDRESS);

const associatedTokenAccount = async (owner: Address, mint: MintInfo): Promise<Address> => {
  const [tokenAccount] = await findAssociatedTokenPda({ owner, mint: mint.address, tokenProgram: mint.tokenProgram });
  return tokenAccount;
};

export const findWrappedSolAccount = async (owner: Address): Promise<Address> => {
  const [tokenAccount] = await findAssociatedTokenPda({
    owner,
    mint: NATIVE_MINT,
    tokenProgram: TOKEN_PROGRAM_ADDRESS,
  });
  return tokenAccount;
};

// Make sure `owner`'s native-mint account holds at least `amount`: create it
// if it does not exist, and top it up from the wallet's SOL by however much
// it lacks. `alreadyWrapped` is what the account holds now.
export const buildWrapSolInstructions = async (
  owner: TransactionSigner,
  amount: bigint,
  alreadyWrapped: bigint,
): Promise<Array<Instruction>> => {
  const wrappedSolAccount = await findWrappedSolAccount(owner.address);
  const instructions: Array<Instruction> = [await buildEnsureWrappedSolAccountInstruction(owner)];
  const shortfall = amount - alreadyWrapped;
  if (shortfall > 0n) {
    instructions.push(
      getTransferSolInstruction({ source: owner, destination: wrappedSolAccount, amount: shortfall }),
      getSyncNativeInstruction({ account: wrappedSolAccount }),
    );
  }
  return instructions;
};

// Create `owner`'s native-mint account if it does not exist; do nothing if
// it does.
const buildEnsureWrappedSolAccountInstruction = (owner: TransactionSigner) =>
  getCreateAssociatedTokenIdempotentInstructionAsync({ payer: owner, owner: owner.address, mint: NATIVE_MINT });

// Close `owner`'s native-mint account, sending everything in it, the wrapped
// SOL and the account's rent deposit, back to the wallet as SOL.
//
// `keepAccount` re-creates the account, empty, in the same transaction. A
// maker with an open offer asking for SOL needs it: take_offer pays into the
// maker's native-mint account and requires it to exist, so closing it for good
// would make the offer impossible to take.
export const buildUnwrapSolInstructions = async (
  owner: TransactionSigner,
  keepAccount = false,
): Promise<Array<Instruction>> => {
  const close = getCloseAccountInstruction({
    account: await findWrappedSolAccount(owner.address),
    destination: owner.address,
    owner,
  });
  return keepAccount ? [close, await buildEnsureWrappedSolAccountInstruction(owner)] : [close];
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
  // What the maker's native-mint account holds now. Only read when the maker
  // offers SOL.
  wrappedSolBalance: bigint;
};

export const buildMakeOfferInstructions = async (request: MakeOfferRequest): Promise<Array<Instruction>> => {
  const { maker, mintA, mintB, id, programAddress } = request;
  // The program passes one token program for both mints, so both must belong
  // to it. The form refuses a mixed pair before it gets here.
  if (mintA.tokenProgram !== mintB.tokenProgram) {
    throw new Error("Both tokens must belong to the same token program");
  }
  const [offer] = await findOfferPda({ maker: maker.address, id }, { programAddress });
  const makeOffer = await getMakeOfferInstructionAsync(
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
  const wrap = isNativeSol(mintA)
    ? await buildWrapSolInstructions(maker, request.offeredAmount, request.wrappedSolBalance)
    : [];
  return [...wrap, makeOffer];
};

// The taker's bounds are the offer as the taker saw it: at least the token A
// the vault held, for at most the token B the offer wanted. If the maker
// swaps the offer for a worse one before this lands, the program refuses it.
export const buildTakeOfferInstructions = async (
  taker: TransactionSigner,
  offer: OfferListing,
  programAddress: Address,
  wrappedSolBalance: bigint,
): Promise<Array<Instruction>> => {
  const takeOffer = getTakeOfferInstruction(
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
  const wrap = isNativeSol(offer.mintB)
    ? await buildWrapSolInstructions(taker, offer.wantedAmount, wrappedSolBalance)
    : [];
  // Receiving SOL: unwrap it. Paying SOL: close the account the payment was
  // wrapped in, now empty, so its rent comes back too. Either way the taker
  // ends with plain SOL and no native-mint account.
  const unwrap = isNativeSol(offer.mintA) || isNativeSol(offer.mintB) ? await buildUnwrapSolInstructions(taker) : [];
  return [...wrap, takeOffer, ...unwrap];
};

export const buildCancelOfferInstructions = async (
  maker: TransactionSigner,
  offer: OfferListing,
  programAddress: Address,
): Promise<Array<Instruction>> => {
  const cancelOffer = getCancelOfferInstruction(
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
  if (!isNativeSol(offer.mintA)) {
    return [cancelOffer];
  }
  // cancel_offer returns the vault's SOL to the maker's native-mint account
  // and requires that account to exist, but the maker may have unwrapped and
  // closed it since making the offer. Re-create it if so, then unwrap.
  return [
    await buildEnsureWrappedSolAccountInstruction(maker),
    cancelOffer,
    ...(await buildUnwrapSolInstructions(maker)),
  ];
};
