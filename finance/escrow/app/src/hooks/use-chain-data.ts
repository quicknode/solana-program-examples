"use client";

import { type Address, address } from "@solana/kit";
import { useRequest } from "@solana/react";
import { useCallback, useMemo } from "react";
import { useRefreshOnChainChange } from "@/components/chain-activity";
import { NATIVE_MINT_ADDRESS, POPULAR_TOKENS } from "@/components/token-picker/popular-token-addresses";
import { fetchAccounts } from "@/lib/accounts";
import { CLUSTER, ESCROW_PROGRAM } from "@/lib/config";
import { fetchAllOffers, fetchOffer } from "@/lib/offers";
import { lookupSolName } from "@/lib/sns";
import { decodeMintInfo } from "@/lib/tokens";
import { fetchWalletTokens } from "@/lib/wallet-tokens";
import { useEscrowClient } from "./use-escrow-client";
import { useWalletConnection } from "./use-wallet";

// Each hook reads one thing from the chain with @solana/react's useRequest,
// which tracks loading and errors and refetches when its source changes, and
// refetches again whenever the chain changes.

export const useOffers = () => {
  const client = useEscrowClient();
  const source = useCallback(
    (abortSignal: AbortSignal) => fetchAllOffers(client.rpc, ESCROW_PROGRAM, abortSignal),
    [client],
  );
  const request = useRequest(source);
  useRefreshOnChainChange(request.refresh);
  return request;
};

export const useOffer = (offerAddress: Address) => {
  const client = useEscrowClient();
  const source = useCallback(
    (abortSignal: AbortSignal) => fetchOffer(client.rpc, ESCROW_PROGRAM, offerAddress, abortSignal),
    [client, offerAddress],
  );
  const request = useRequest(source);
  useRefreshOnChainChange(request.refresh);
  return request;
};

// Whether the escrow program exists at the configured address on this
// cluster. Without it every list is empty for a reason no list can explain.
export const useProgramDeployed = () => {
  const client = useEscrowClient();
  const source = useCallback(
    async (abortSignal: AbortSignal) => {
      const { value } = await client.rpc.getAccountInfo(ESCROW_PROGRAM, { encoding: "base64" }).send({ abortSignal });
      return value?.executable === true;
    },
    [client],
  );
  return useRequest(source);
};

export const useConnectedAddress = (): Address | null => {
  const connectedAddress = useWalletConnection()?.account.address;
  return useMemo(() => (connectedAddress ? address(connectedAddress) : null), [connectedAddress]);
};

export const useWalletTokens = () => {
  const client = useEscrowClient();
  const owner = useConnectedAddress();
  const source = useMemo(
    () => (owner ? (abortSignal: AbortSignal) => fetchWalletTokens(client.rpc, owner, abortSignal) : null),
    [client, owner],
  );
  const request = useRequest(source);
  useRefreshOnChainChange(request.refresh);
  return request;
};

export const useSolBalance = () => {
  const client = useEscrowClient();
  const owner = useConnectedAddress();
  const source = useMemo(
    () =>
      owner
        ? async (abortSignal: AbortSignal) => {
            const { value } = await client.rpc.getBalance(owner, { commitment: "confirmed" }).send({ abortSignal });
            return value;
          }
        : null,
    [client, owner],
  );
  const request = useRequest(source);
  useRefreshOnChainChange(request.refresh);
  return request;
};

// The popular tokens for this cluster that really are mints here, in the
// list's order, plus the native mint (SOL), which every cluster has. A listed
// address with no mint behind it on this cluster is left out rather than
// offered.
export const usePopularMints = () => {
  const client = useEscrowClient();
  const source = useCallback(
    async (abortSignal: AbortSignal) => {
      const presets = POPULAR_TOKENS[CLUSTER];
      const addresses = [...new Set([NATIVE_MINT_ADDRESS, ...presets.map((token) => token.address)])].map((value) =>
        address(value),
      );
      const accounts = await fetchAccounts(client.rpc, addresses, abortSignal);
      const mintInfoAt = (value: string) => decodeMintInfo(address(value), accounts.get(address(value)) ?? null);
      return {
        presets: presets.flatMap((token) => {
          const mint = mintInfoAt(token.address);
          return mint ? [{ token, mint }] : [];
        }),
        nativeMint: mintInfoAt(NATIVE_MINT_ADDRESS),
      };
    },
    [client],
  );
  return useRequest(source);
};

// The connected wallet's .sol name, or null when it has none or the lookup
// fails: a name is a nicety, and the address always stands in for it.
export const useSolName = () => {
  const owner = useConnectedAddress();
  const source = useMemo(
    () =>
      owner
        ? async () => {
            try {
              return await lookupSolName(owner);
            } catch {
              return null;
            }
          }
        : null,
    [owner],
  );
  return useRequest(source).data ?? null;
};
