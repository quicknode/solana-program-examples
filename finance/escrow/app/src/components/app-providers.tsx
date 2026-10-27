"use client";

import { ClientProvider } from "@solana/react";
import { type ReactNode, useState } from "react";
import { createEscrowClient } from "@/lib/client";
import { ChainActivityProvider } from "./chain-activity";

// Builds the app's one Kit client and publishes it to every component below.
// The client lives as long as the page does: it holds the wallet connection,
// so it is built once rather than on every render.
export const AppProviders = ({ children }: { children: ReactNode }) => {
  const [client] = useState(createEscrowClient);
  return (
    <ClientProvider client={client}>
      <ChainActivityProvider>{children}</ChainActivityProvider>
    </ClientProvider>
  );
};
