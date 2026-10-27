import {
  type Address,
  type Base58EncodedBytes,
  type Base64EncodedDataResponse,
  type GetAccountInfoApi,
  type GetMultipleAccountsApi,
  type GetProgramAccountsApi,
  getBase58Decoder,
  getBase64Encoder,
  type Rpc,
} from "@solana/kit";
import { findAssociatedTokenPda } from "@solana-program/token";
import { getOfferDecoder, getOfferSize, OFFER_DISCRIMINATOR, type Offer } from "../generated";
import { fetchAccounts } from "./accounts";
import { unitPrice } from "./amounts";
import { decodeMintInfo, decodeTokenAmount, type MintInfo } from "./tokens";

export type OfferReadingRpc = Rpc<GetProgramAccountsApi & GetMultipleAccountsApi & GetAccountInfoApi>;

// An open offer, as a taker sees it: the offer account's own fields, plus the
// two things the offer account does not store. The amount on offer is
// whatever the vault holds, because take_offer pays out the vault's whole
// balance. And each mint's decimals and name, without which an amount is just
// a large integer.
export type OfferListing = {
  address: Address;
  id: bigint;
  maker: Address;
  mintA: MintInfo;
  mintB: MintInfo;
  vault: Address;
  // Token A in the vault: what a taker receives.
  offeredAmount: bigint;
  // Token B the maker wants: what a taker pays.
  wantedAmount: bigint;
  // How much token B one whole token A costs a taker, and the reverse.
  priceOfAInB: number;
  priceOfBInA: number;
};

export const findVaultAddress = async (offerAddress: Address, mint: MintInfo): Promise<Address> => {
  const [vault] = await findAssociatedTokenPda({
    owner: offerAddress,
    mint: mint.address,
    tokenProgram: mint.tokenProgram,
  });
  return vault;
};

// Join decoded offers with their mints and vault balances. An offer whose
// mint cannot be read, or whose vault is gone, is left out: it is not an
// offer anyone can take.
const buildListings = async (
  rpc: OfferReadingRpc,
  offers: Array<{ address: Address; offer: Offer }>,
  abortSignal?: AbortSignal,
): Promise<Array<OfferListing>> => {
  const mintAddresses = [...new Set(offers.flatMap(({ offer }) => [offer.tokenMintA, offer.tokenMintB]))];
  const mintAccounts = await fetchAccounts(rpc, mintAddresses, abortSignal);
  const mints = new Map<Address, MintInfo>();
  for (const [mintAddress, account] of mintAccounts) {
    const mint = decodeMintInfo(mintAddress, account);
    if (mint) {
      mints.set(mintAddress, mint);
    }
  }

  const readable = await Promise.all(
    offers.flatMap(({ address: offerAddress, offer }) => {
      const mintA = mints.get(offer.tokenMintA);
      const mintB = mints.get(offer.tokenMintB);
      if (!mintA || !mintB) {
        return [];
      }
      return [findVaultAddress(offerAddress, mintA).then((vault) => ({ offerAddress, offer, mintA, mintB, vault }))];
    }),
  );

  const vaultAccounts = await fetchAccounts(
    rpc,
    readable.map(({ vault }) => vault),
    abortSignal,
  );

  return readable.flatMap(({ offerAddress, offer, mintA, mintB, vault }) => {
    const offeredAmount = decodeTokenAmount(vaultAccounts.get(vault) ?? null);
    if (offeredAmount === null) {
      return [];
    }
    return [
      {
        address: offerAddress,
        id: offer.id,
        maker: offer.maker,
        mintA,
        mintB,
        vault,
        offeredAmount,
        wantedAmount: offer.tokenBWantedAmount,
        priceOfAInB: unitPrice(offeredAmount, mintA.decimals, offer.tokenBWantedAmount, mintB.decimals),
        priceOfBInA: unitPrice(offer.tokenBWantedAmount, mintB.decimals, offeredAmount, mintA.decimals),
      },
    ];
  });
};

const decodeOffer = (data: Base64EncodedDataResponse): Offer =>
  getOfferDecoder().decode(getBase64Encoder().encode(data[0]));

// Every open offer. Offer accounts are found by the two things every one of
// them shares: the eight-byte discriminator Anchor writes at the start of the
// account, and the account's size.
export const fetchAllOffers = async (
  rpc: OfferReadingRpc,
  programAddress: Address,
  abortSignal?: AbortSignal,
): Promise<Array<OfferListing>> => {
  const accounts = await rpc
    .getProgramAccounts(programAddress, {
      encoding: "base64",
      commitment: "confirmed",
      filters: [
        { dataSize: BigInt(getOfferSize()) },
        {
          memcmp: {
            offset: 0n,
            bytes: getBase58Decoder().decode(OFFER_DISCRIMINATOR) as Base58EncodedBytes,
            encoding: "base58",
          },
        },
      ],
    })
    .send({ abortSignal });
  const offers = accounts.map(({ pubkey, account }) => ({ address: pubkey, offer: decodeOffer(account.data) }));
  return buildListings(rpc, offers, abortSignal);
};

// One offer, or null when no offer lives at that address: it was taken,
// cancelled, or never existed.
export const fetchOffer = async (
  rpc: OfferReadingRpc,
  programAddress: Address,
  offerAddress: Address,
  abortSignal?: AbortSignal,
): Promise<OfferListing | null> => {
  const { value } = await rpc
    .getAccountInfo(offerAddress, { encoding: "base64", commitment: "confirmed" })
    .send({ abortSignal });
  if (value === null || value.owner !== programAddress) {
    return null;
  }
  const [listing] = await buildListings(rpc, [{ address: offerAddress, offer: decodeOffer(value.data) }], abortSignal);
  return listing ?? null;
};

export type OfferSort = "best-price" | "largest" | "smallest";

// Offers grouped the way a taker compares them: the same pair of tokens
// together, and within a pair the cheapest token A first.
export const sortOffers = (offers: Array<OfferListing>, sort: OfferSort): Array<OfferListing> => {
  const pairKey = (offer: OfferListing) => `${offer.mintA.symbol}/${offer.mintB.symbol}`;
  return [...offers].sort((first, second) => {
    const byPair = pairKey(first).localeCompare(pairKey(second));
    if (byPair !== 0) {
      return byPair;
    }
    if (sort === "best-price") {
      return first.priceOfAInB - second.priceOfAInB;
    }
    const difference = first.offeredAmount - second.offeredAmount;
    const ascending = difference < 0n ? -1 : difference > 0n ? 1 : 0;
    return sort === "largest" ? -ascending : ascending;
  });
};
