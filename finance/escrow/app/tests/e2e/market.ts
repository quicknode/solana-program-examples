import {
  type Address,
  createKeyPairSignerFromPrivateKeyBytes,
  getAddressDecoder,
  type KeyPairSigner,
} from "@solana/kit";
import { findAssociatedTokenPda } from "@solana-program/token";
import { TOKEN_2022_PROGRAM_ADDRESS } from "@solana-program/token-2022";
import { ESCROW_PROGRAM_ADDRESS, findOfferPda, getOfferEncoder } from "../../src/generated";
import type { FakeAccount } from "../fake-rpc";
import { mintAccount, tokenAccount } from "../fixtures";

// The market the end-to-end tests open the app onto: three makers' offers in
// the book's tokens, one offer of the viewer's own, and a viewer, Bob, whose
// wallet holds 1,000 tUSDC, 1.5 tTSLAx and 1 SOL.

const TOKEN_PROGRAM = TOKEN_2022_PROGRAM_ADDRESS;
const ONE_SOL = 1_000_000_000n;

const stableAddress = (byte: number): Address => getAddressDecoder().decode(new Uint8Array(32).fill(byte));

export const MINTS = {
  usdc: { address: stableAddress(11), decimals: 6, name: "Test USDC", symbol: "tUSDC" },
  tslax: { address: stableAddress(12), decimals: 8, name: "Test TSLAx", symbol: "tTSLAx" },
  nvdax: { address: stableAddress(13), decimals: 8, name: "Test NVDAx", symbol: "tNVDAx" },
};

export const MAKERS = { alice: stableAddress(21), carol: stableAddress(22), dave: stableAddress(23) };

export type Market = {
  accounts: Map<Address, FakeAccount & { lamports?: bigint }>;
  bob: KeyPairSigner;
  bobPrivateKey: Uint8Array;
  offers: { alice: Address; carol: Address; dave: Address; bob: Address };
};

const associated = async (owner: Address, mint: Address) =>
  (await findAssociatedTokenPda({ owner, mint, tokenProgram: TOKEN_PROGRAM }))[0];

export const buildMarket = async (): Promise<Market> => {
  const bobPrivateKey = new Uint8Array(32).fill(42);
  const bob = await createKeyPairSignerFromPrivateKeyBytes(bobPrivateKey);
  const accounts = new Map<Address, FakeAccount & { lamports?: bigint }>();

  for (const mint of Object.values(MINTS)) {
    accounts.set(mint.address, mintAccount(mint.decimals, TOKEN_PROGRAM, mint));
  }

  const holdTokens = async (owner: Address, mint: Address, amount: bigint) => {
    accounts.set(await associated(owner, mint), tokenAccount(mint, owner, amount, TOKEN_PROGRAM));
  };

  const addOffer = async (
    maker: Address,
    id: bigint,
    mintA: Address,
    offered: bigint,
    mintB: Address,
    wanted: bigint,
  ): Promise<Address> => {
    const [offer, bump] = await findOfferPda({ maker, id });
    accounts.set(offer, {
      owner: ESCROW_PROGRAM_ADDRESS,
      data: getOfferEncoder().encode({
        id,
        maker,
        tokenMintA: mintA,
        tokenMintB: mintB,
        tokenBWantedAmount: wanted,
        bump,
      }),
    });
    await holdTokens(offer, mintA, offered);
    // make_offer creates the maker's token-B account.
    await holdTokens(maker, mintB, 0n);
    return offer;
  };

  const offers = {
    // The book's offer: 250 USDC for 1 TSLAx.
    alice: await addOffer(MAKERS.alice, 1n, MINTS.usdc.address, 250_000_000n, MINTS.tslax.address, 100_000_000n),
    // 500 USDC for 2.5 TSLAx: 200 USDC a TSLAx, a worse deal for a taker.
    carol: await addOffer(MAKERS.carol, 7n, MINTS.usdc.address, 500_000_000n, MINTS.tslax.address, 250_000_000n),
    // 3 NVDAx for 540 USDC.
    dave: await addOffer(MAKERS.dave, 3n, MINTS.nvdax.address, 300_000_000n, MINTS.usdc.address, 540_000_000n),
    // Bob's own: 0.5 TSLAx for 130 USDC.
    bob: await addOffer(bob.address, 9n, MINTS.tslax.address, 50_000_000n, MINTS.usdc.address, 130_000_000n),
  };

  await holdTokens(bob.address, MINTS.usdc.address, 1_000_000_000n);
  await holdTokens(bob.address, MINTS.tslax.address, 150_000_000n);
  accounts.set(bob.address, {
    owner: "11111111111111111111111111111111" as Address,
    data: new Uint8Array(),
    lamports: ONE_SOL,
  });

  return { accounts, bob, bobPrivateKey, offers };
};
