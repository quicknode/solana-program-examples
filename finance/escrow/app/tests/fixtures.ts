import { type Address, none, some } from "@solana/kit";
import { AccountState, getMintEncoder, getTokenEncoder } from "@solana-program/token-2022";
import type { FakeAccount } from "./fake-rpc";
import { testAddress } from "./fake-rpc";

// Mint and token account data as the token programs lay it out. Token-2022's
// encoders write classic token program accounts too, when given no
// extensions.

export const mintAccount = (
  decimals: number,
  owner: Address,
  metadata?: { name: string; symbol: string },
  mintAuthority: Address | null = null,
): FakeAccount => ({
  owner,
  data: getMintEncoder().encode({
    mintAuthority: mintAuthority ? some(mintAuthority) : none(),
    supply: 1_000_000_000_000n,
    decimals,
    isInitialized: true,
    freezeAuthority: none(),
    extensions: metadata
      ? some([
          {
            __kind: "TokenMetadata",
            updateAuthority: none(),
            mint: testAddress(99),
            name: metadata.name,
            symbol: metadata.symbol,
            uri: "",
            additionalMetadata: new Map(),
          },
        ])
      : none(),
  }),
});

export const tokenAccount = (mint: Address, owner: Address, amount: bigint, program: Address): FakeAccount => ({
  owner: program,
  data: getTokenEncoder().encode({
    mint,
    owner,
    amount,
    delegate: none(),
    state: AccountState.Initialized,
    isNative: none(),
    delegatedAmount: 0n,
    closeAuthority: none(),
    extensions: none(),
  }),
});
