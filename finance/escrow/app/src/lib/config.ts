import { type Address, address } from "@solana/kit";
import { ESCROW_PROGRAM_ADDRESS } from "../generated";

// Every setting comes from a NEXT_PUBLIC_ environment variable, so one build
// can point at localnet, devnet or mainnet. Next.js inlines these at build
// time; see .env.example.

export type Cluster = "localnet" | "devnet" | "testnet" | "mainnet";

const CLUSTERS: Array<Cluster> = ["localnet", "devnet", "testnet", "mainnet"];

const DEFAULT_RPC_URLS: Record<Cluster, string> = {
  localnet: "http://127.0.0.1:8899",
  devnet: "https://api.devnet.solana.com",
  testnet: "https://api.testnet.solana.com",
  mainnet: "https://api.mainnet-beta.solana.com",
};

const readCluster = (value: string | undefined): Cluster => {
  if (value === undefined || value === "") {
    return "devnet";
  }
  if (!CLUSTERS.includes(value as Cluster)) {
    throw new Error(`NEXT_PUBLIC_CLUSTER must be one of ${CLUSTERS.join(", ")}, not "${value}"`);
  }
  return value as Cluster;
};

export const CLUSTER: Cluster = readCluster(process.env.NEXT_PUBLIC_CLUSTER);

export const RPC_URL: string = process.env.NEXT_PUBLIC_RPC_URL || DEFAULT_RPC_URLS[CLUSTER];

// Unset means the RPC URL with its protocol switched to ws(s), which the RPC
// plugin works out for itself.
export const RPC_SUBSCRIPTIONS_URL: string | undefined = process.env.NEXT_PUBLIC_RPC_SUBSCRIPTIONS_URL || undefined;

// The address the program is deployed at. Defaults to the address in the IDL,
// which is the address in the program's declare_id!.
export const ESCROW_PROGRAM: Address = process.env.NEXT_PUBLIC_ESCROW_PROGRAM_ADDRESS
  ? address(process.env.NEXT_PUBLIC_ESCROW_PROGRAM_ADDRESS)
  : ESCROW_PROGRAM_ADDRESS;

// The Wallet Standard chain a wallet must support to be offered.
export const WALLET_CHAIN = `solana:${CLUSTER}` as const;

// Test clusters hand out SOL, and the app offers to mint test tokens there.
export const IS_TEST_CLUSTER = CLUSTER !== "mainnet";

export const explorerUrl = (kind: "address" | "tx", value: string): string => {
  const url = new URL(`https://explorer.solana.com/${kind}/${value}`);
  if (CLUSTER === "localnet") {
    url.searchParams.set("cluster", "custom");
    url.searchParams.set("customUrl", RPC_URL);
  } else if (CLUSTER !== "mainnet") {
    url.searchParams.set("cluster", CLUSTER);
  }
  return url.toString();
};
