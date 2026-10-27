"use client";

import { createContext, type ReactNode, useCallback, useContext, useEffect, useRef, useState } from "react";
import { useEscrowClient } from "@/hooks/use-escrow-client";
import { ESCROW_PROGRAM } from "@/lib/config";

// Keeps what is on screen in step with the chain. Components that read chain
// data register a refresh function here, and every one of them runs when
// either this tab confirms a transaction, or the escrow program's accounts
// change: someone, anywhere, made, took or cancelled an offer.

type ChainActivity = {
  // Call after this tab confirms a transaction.
  announceChange: () => void;
  // Run `refresh` whenever the chain changes; returns the unregister function.
  onChange: (refresh: () => void) => () => void;
  // Whether the app is hearing about other people's offers as they happen.
  isLive: boolean;
};

const ChainActivityContext = createContext<ChainActivity | null>(null);

// Several account changes arrive together when one transaction touches them
// all, so wait for the burst to finish before refreshing once.
const SETTLE_MILLISECONDS = 400;

export const ChainActivityProvider = ({ children }: { children: ReactNode }) => {
  const client = useEscrowClient();
  const listeners = useRef(new Set<() => void>());
  const settleTimer = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
  const [isLive, setIsLive] = useState(false);

  const announceChange = useCallback(() => {
    clearTimeout(settleTimer.current);
    settleTimer.current = setTimeout(() => {
      for (const refresh of listeners.current) {
        refresh();
      }
    }, SETTLE_MILLISECONDS);
  }, []);

  const onChange = useCallback((refresh: () => void) => {
    listeners.current.add(refresh);
    return () => {
      listeners.current.delete(refresh);
    };
  }, []);

  useEffect(() => {
    const abortController = new AbortController();
    const listen = async () => {
      try {
        const notifications = await client.rpcSubscriptions
          .programNotifications(ESCROW_PROGRAM, { commitment: "confirmed", encoding: "base64" })
          .subscribe({ abortSignal: abortController.signal });
        setIsLive(true);
        for await (const _notification of notifications) {
          announceChange();
        }
      } catch {
        // No WebSocket endpoint, or it dropped: the app still works, it just
        // refreshes after this tab's own transactions and on request.
      } finally {
        if (!abortController.signal.aborted) {
          setIsLive(false);
        }
      }
    };
    listen();
    return () => abortController.abort();
  }, [client, announceChange]);

  return (
    <ChainActivityContext.Provider value={{ announceChange, onChange, isLive }}>
      {children}
    </ChainActivityContext.Provider>
  );
};

export const useChainActivity = (): ChainActivity => {
  const activity = useContext(ChainActivityContext);
  if (!activity) {
    throw new Error("useChainActivity needs a ChainActivityProvider above it");
  }
  return activity;
};

// Re-run `refresh` whenever the chain changes.
export const useRefreshOnChainChange = (refresh: () => void) => {
  const { onChange } = useChainActivity();
  useEffect(() => onChange(refresh), [onChange, refresh]);
};
