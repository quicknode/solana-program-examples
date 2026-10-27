"use client";

import type { Signature } from "@solana/kit";
import { useAction } from "@solana/react";
import { useChainActivity } from "@/components/chain-activity";
import type { EscrowClient } from "@/lib/client";
import { useEscrowClient } from "./use-escrow-client";

export type TransactionInput = Parameters<EscrowClient["sendTransactions"]>[0];

// Sends one or more instructions as transactions, paid for and signed by the
// connected wallet, and tracks the attempt as state a component can render:
// `isRunning` while the wallet prompts and the network confirms, then `data`
// (the last transaction's signature) or `error`.
//
// `build` runs when the user clicks, not when the component renders, so the
// instructions it builds use the wallet and the offer as they are at that
// moment.
export const useTransaction = () => {
  const client = useEscrowClient();
  const { announceChange } = useChainActivity();
  return useAction(async (abortSignal, build: () => Promise<TransactionInput>): Promise<Signature> => {
    const input = await build();
    const result = await client.sendTransactions(input, { abortSignal });
    announceChange();
    const signatures = flattenSignatures(result);
    const lastSignature = signatures.at(-1);
    if (!lastSignature) {
      throw new Error("The network confirmed no transaction");
    }
    return lastSignature;
  });
};

type PlanResult = Awaited<ReturnType<EscrowClient["sendTransactions"]>>;

const flattenSignatures = (result: PlanResult): Array<Signature> => {
  if (result.kind === "single") {
    return result.status === "successful" ? [result.context.signature] : [];
  }
  return result.plans.flatMap((plan) => flattenSignatures(plan));
};
