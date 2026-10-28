import {
  type Address,
  address,
  createKeyPairSignerFromPrivateKeyBytes,
  getAddressDecoder,
  getAddressEncoder,
  getUtf8Encoder,
  type KeyPairSigner,
} from "@solana/kit";
import {
  getReverseAddressFromDomainAddress,
  getSnsDomainAddress,
  NAME_OFFERS_ADDRESS,
  NAME_PROGRAM_ADDRESS,
  PrimaryDomainState,
  SNS_ROOT_DOMAIN_ACCOUNT,
} from "@solana-name-service/sns-sdk-kit";
import { findAssociatedTokenPda, TOKEN_PROGRAM_ADDRESS } from "@solana-program/token";
import { TOKEN_2022_PROGRAM_ADDRESS } from "@solana-program/token-2022";
import { NATIVE_MINT_ADDRESS, POPULAR_TOKENS } from "../../src/components/token-picker/popular-token-addresses";
import { ESCROW_PROGRAM_ADDRESS, findOfferPda, getOfferEncoder } from "../../src/generated";
import type { FakeAccount } from "../fake-rpc";
import { mintAccount, tokenAccount } from "../fixtures";

// The market the end-to-end tests open the app onto, on a mock of devnet.
//
// Two kinds of token. The book's tokens as Token-2022 test tokens (tUSDC,
// tTSLAx, tNVDAx), which the app's Test tokens page would create. And three
// of the devnet presets at their real devnet addresses: SOL (the native
// mint), USDC (a classic token program mint) and PYUSD (Token-2022). The
// other presets are left out on purpose, so the tests see the app hide a
// preset that is not a mint on the cluster.
//
// The viewer, Bob, holds 1,000 tUSDC, 1.5 tTSLAx, 500 USDC and 5 SOL, plus
// 0.25 SOL wrapped in his SOL token account. Three
// makers have offers in the test tokens, two in SOL and USDC, and Bob has
// one of his own.

const ONE_SOL = 1_000_000_000n;

const stableAddress = (byte: number): Address => getAddressDecoder().decode(new Uint8Array(32).fill(byte));

const devnetPreset = (symbol: string): Address => {
  const token = POPULAR_TOKENS.devnet.find((preset) => preset.symbol === symbol);
  if (!token) {
    throw new Error(`No devnet preset ${symbol}`);
  }
  return address(token.address);
};

export const MINTS = {
  usdc: { address: stableAddress(11), decimals: 6, name: "Test USDC", symbol: "tUSDC" },
  tslax: { address: stableAddress(12), decimals: 8, name: "Test TSLAx", symbol: "tTSLAx" },
  nvdax: { address: stableAddress(13), decimals: 8, name: "Test NVDAx", symbol: "tNVDAx" },
};

export const PRESET_MINTS = {
  sol: address(NATIVE_MINT_ADDRESS),
  usdc: devnetPreset("USDC"),
  pyusd: devnetPreset("PYUSD"),
};

export const MAKERS = {
  alice: stableAddress(21),
  carol: stableAddress(22),
  dave: stableAddress(23),
  erin: stableAddress(24),
  frank: stableAddress(25),
};

type MarketAccount = FakeAccount & { lamports?: bigint };

export type Market = {
  accounts: Map<Address, MarketAccount>;
  bob: KeyPairSigner;
  bobPrivateKey: Uint8Array;
  offers: { alice: Address; carol: Address; dave: Address; bob: Address; erin: Address; frank: Address };
};

const rentFor = (size: number) => BigInt((size + 128) * 6960);

export const buildMarket = async (): Promise<Market> => {
  const bobPrivateKey = new Uint8Array(32).fill(42);
  const bob = await createKeyPairSignerFromPrivateKeyBytes(bobPrivateKey);
  const accounts = new Map<Address, MarketAccount>();

  for (const mint of Object.values(MINTS)) {
    accounts.set(mint.address, mintAccount(mint.decimals, TOKEN_2022_PROGRAM_ADDRESS, mint));
  }
  accounts.set(PRESET_MINTS.sol, mintAccount(9, TOKEN_PROGRAM_ADDRESS));
  accounts.set(PRESET_MINTS.usdc, mintAccount(6, TOKEN_PROGRAM_ADDRESS));
  accounts.set(PRESET_MINTS.pyusd, mintAccount(6, TOKEN_2022_PROGRAM_ADDRESS, { name: "PayPal USD", symbol: "PYUSD" }));

  const programOf = (mint: Address) =>
    accounts.get(mint)?.owner === TOKEN_PROGRAM_ADDRESS ? TOKEN_PROGRAM_ADDRESS : TOKEN_2022_PROGRAM_ADDRESS;

  // A token account at the owner's associated address. A native-mint account
  // holds its tokens as lamports, on top of its rent.
  const holdTokens = async (owner: Address, mint: Address, amount: bigint) => {
    const tokenProgram = programOf(mint);
    const [tokenAccountAddress] = await findAssociatedTokenPda({ owner, mint, tokenProgram });
    const account = tokenAccount(mint, owner, amount, tokenProgram);
    accounts.set(tokenAccountAddress, {
      ...account,
      lamports: rentFor(account.data.length) + (mint === PRESET_MINTS.sol ? amount : 0n),
    });
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
    accounts.set(maker, {
      owner: "11111111111111111111111111111111" as Address,
      data: new Uint8Array(),
      lamports: ONE_SOL,
    });
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
    // 2 SOL for 300 USDC: a taker receives SOL.
    erin: await addOffer(MAKERS.erin, 4n, PRESET_MINTS.sol, 2n * ONE_SOL, PRESET_MINTS.usdc, 300_000_000n),
    // 100 USDC for 0.5 SOL: a taker pays SOL.
    frank: await addOffer(MAKERS.frank, 5n, PRESET_MINTS.usdc, 100_000_000n, PRESET_MINTS.sol, ONE_SOL / 2n),
  };

  await holdTokens(bob.address, MINTS.usdc.address, 1_000_000_000n);
  await holdTokens(bob.address, MINTS.tslax.address, 150_000_000n);
  await holdTokens(bob.address, PRESET_MINTS.usdc, 500_000_000n);
  // 0.25 SOL a taker paid Bob earlier, still wrapped in his SOL token account.
  await holdTokens(bob.address, PRESET_MINTS.sol, ONE_SOL / 4n);
  accounts.set(bob.address, {
    owner: "11111111111111111111111111111111" as Address,
    data: new Uint8Array(),
    lamports: 5n * ONE_SOL,
  });

  await giveSolName(accounts, bob.address, "bob");

  return { accounts, bob, bobPrivateKey, offers };
};

// Make `name`.sol `owner`'s primary SNS name, with the three accounts the SNS
// SDK reads to find it: the owner's primary-domain record pointing at the
// domain, the domain's registry naming its owner, and the reverse-lookup
// account holding the name's text.
const giveSolName = async (accounts: Map<Address, MarketAccount>, owner: Address, name: string) => {
  const { domainAddress } = await getSnsDomainAddress({ domain: name });
  const addressBytes = (value: Address) => [...getAddressEncoder().encode(value)];
  const registryHeader = (registryOwner: Address) => [
    ...addressBytes(SNS_ROOT_DOMAIN_ACCOUNT),
    ...addressBytes(registryOwner),
    ...new Array(32).fill(0),
  ];
  const nameBytes = [...getUtf8Encoder().encode(name)];
  const nameLength = new Uint8Array(new Uint32Array([nameBytes.length]).buffer);

  accounts.set(await PrimaryDomainState.getAddress(NAME_OFFERS_ADDRESS, owner), {
    owner: NAME_OFFERS_ADDRESS,
    data: new Uint8Array([1, ...addressBytes(domainAddress)]),
  });
  accounts.set(domainAddress, { owner: NAME_PROGRAM_ADDRESS, data: new Uint8Array(registryHeader(owner)) });
  accounts.set(await getReverseAddressFromDomainAddress({ domainAddress }), {
    owner: NAME_PROGRAM_ADDRESS,
    data: new Uint8Array([...registryHeader(owner), ...nameLength, ...nameBytes]),
  });
};
