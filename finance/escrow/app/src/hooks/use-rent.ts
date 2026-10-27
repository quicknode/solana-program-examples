"use client";

import { useRequest } from "@solana/react";
import { getTokenSize } from "@solana-program/token-2022";
import { useCallback } from "react";
import { getOfferSize } from "@/generated";
import { useEscrowClient } from "./use-escrow-client";

// The rent deposits the escrow's accounts hold, in lamports. Every account
// must hold enough lamports for its size to be rent-exempt; whoever creates
// the account pays that deposit, and gets it back when the account closes.
export const useEscrowRent = () => {
  const client = useEscrowClient();
  const source = useCallback(async () => {
    const [offerAccount, classicTokenAccount, token2022Account] = await Promise.all([
      client.getMinimumBalance(getOfferSize()),
      client.getMinimumBalance(getTokenSize()),
      // An associated token account under Token-2022 carries the
      // ImmutableOwner extension, which makes it a few bytes larger.
      client.getMinimumBalance(getTokenSize([{ __kind: "ImmutableOwner" }])),
    ]);
    return { offerAccount, classicTokenAccount, token2022Account };
  }, [client]);
  return useRequest(source);
};
