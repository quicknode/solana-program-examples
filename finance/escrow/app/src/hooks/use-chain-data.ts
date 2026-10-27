"use client";

import { type Address, address } from "@solana/kit";
import { useRequest } from "@solana/react";
import { useCallback, useMemo } from "react";
import { useRefreshOnChainChange } from "@/components/chain-activity";
import { ESCROW_PROGRAM } from "@/lib/config";
import { fetchAllOffers, fetchOffer } from "@/lib/offers";
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
