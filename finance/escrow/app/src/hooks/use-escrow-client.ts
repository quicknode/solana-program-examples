import { useClient } from "@solana/react";
import type { EscrowClient } from "@/lib/client";

// The client AppProviders built, typed with every plugin it was built from.
export const useEscrowClient = () => useClient<EscrowClient>();
