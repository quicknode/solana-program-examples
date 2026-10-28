// Checks every popular-token address against a cluster: each must be a mint
// there. Prints each one's symbol, token program and decimals, and exits
// non-zero if any address holds no mint.
//
//   pnpm check-popular-tokens --cluster devnet
//   pnpm check-popular-tokens --cluster mainnet --rpc https://your-endpoint
//
// Run it whenever popular-token-addresses.ts changes. The app makes the same
// check every time it loads, and hides a token that fails it; this script
// makes the failure loud instead.
import { parseArgs } from "node:util";
import { address, createSolanaRpc } from "@solana/kit";
import { POPULAR_TOKENS } from "../src/components/token-picker/popular-token-addresses";
import { fetchAccounts } from "../src/lib/accounts";
import type { Cluster } from "../src/lib/config";
import { decodeMintInfo } from "../src/lib/tokens";

const PUBLIC_RPC_URLS: Partial<Record<Cluster, string>> = {
  devnet: "https://api.devnet.solana.com",
  mainnet: "https://api.mainnet-beta.solana.com",
};

const TOKEN_PROGRAM_NAMES: Record<string, string> = {
  TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA: "Token",
  TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb: "Token-2022",
};

const { values } = parseArgs({
  options: { cluster: { type: "string", default: "devnet" }, rpc: { type: "string" } },
});
const cluster = values.cluster as Cluster;
const tokens = POPULAR_TOKENS[cluster];
if (!tokens) {
  throw new Error(`--cluster must be one of ${Object.keys(POPULAR_TOKENS).join(", ")}`);
}
const rpcUrl = values.rpc ?? PUBLIC_RPC_URLS[cluster];
if (!rpcUrl) {
  throw new Error(`Pass --rpc: there is no public RPC URL for ${cluster}`);
}

const rpc = createSolanaRpc(rpcUrl);
const accounts = await fetchAccounts(
  rpc,
  tokens.map((token) => address(token.address)),
);

let failures = 0;
for (const token of tokens) {
  const mint = decodeMintInfo(address(token.address), accounts.get(address(token.address)) ?? null);
  if (mint) {
    console.log(
      `ok    ${token.symbol.padEnd(8)} ${token.address}  ${TOKEN_PROGRAM_NAMES[mint.tokenProgram]}, ${mint.decimals} decimals`,
    );
  } else {
    failures += 1;
    console.log(`FAIL  ${token.symbol.padEnd(8)} ${token.address}  is not a mint on ${cluster}`);
  }
}
console.log(`${tokens.length - failures} of ${tokens.length} popular tokens are mints on ${cluster}`);
process.exitCode = failures === 0 ? 0 : 1;
