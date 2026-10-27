"use client";

import { useConnectedWallet, useWalletStatus } from "@solana/kit-plugin-wallet/react";
import { useSyncExternalStore } from "react";
import { useEscrowClient } from "./use-escrow-client";

const subscribeToNothing = () => () => {};

// False while React hydrates the server's HTML, true afterwards. The server
// renders every page with no wallet, because wallets live in the browser; a
// browser that has already reconnected one by the time React hydrates would
// otherwise render something different from that HTML, which React rejects.
export const useHydrated = () =>
  useSyncExternalStore(
    subscribeToNothing,
    () => true,
    () => false,
  );

// The connected wallet (account, signer and wallet), or null when there is
// none, or while the page is still hydrating.
export const useWalletConnection = () => {
  const client = useEscrowClient();
  const connected = useConnectedWallet(client);
  return useHydrated() ? connected : null;
};

// The wallet plugin's status, reported as "pending" until hydration is over,
// which is what the server rendered.
export const useHydratedWalletStatus = () => {
  const client = useEscrowClient();
  const status = useWalletStatus(client);
  return useHydrated() ? status : "pending";
};
