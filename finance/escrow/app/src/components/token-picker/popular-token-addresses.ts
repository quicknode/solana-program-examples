import type { Cluster } from "@/lib/config";

// The well-known tokens a maker can ask for in one click, per cluster. Data
// only: every mint address the app knows by name is written here, once.
//
// The rule for adding one: the comment above an address names the
// issuer-controlled page that publishes it, and the network that page lists
// it under. An address nobody can prove from such a page does not go in. A
// listed address still has to be a mint on the cluster the app talks to:
// the app checks each one when it loads and hides any that is not, and
// `pnpm check-popular-tokens` reports the same thing from the command line.
//
// Tokenized stocks appear on mainnet only once their addresses can be proved
// from the issuer's own list; no issuer publishes devnet mints of them.

export type PopularToken = {
  address: string;
  symbol: string;
  name: string;
  // The issuer-controlled page that publishes this address for this cluster.
  source: string;
  // A logo in public/tokens, from the issuer's own brand assets; see
  // public/tokens/SOURCES.md. Absent when no official logo is bundled.
  logo?: string;
};

// The token program's native mint: holding it means holding SOL, wrapped in a
// token account. One address on every cluster.
// Proof: `declare_id!` in the token program's native_mint module,
// https://github.com/solana-program/token/blob/main/interface/src/native_mint.rs
export const NATIVE_MINT_ADDRESS = "So11111111111111111111111111111111111111112";

const SOL: PopularToken = {
  address: NATIVE_MINT_ADDRESS,
  symbol: "SOL",
  name: "Solana",
  source: "https://github.com/solana-program/token/blob/main/interface/src/native_mint.rs",
};

const DEVNET: Array<PopularToken> = [
  SOL,
  {
    // Circle, "USDC contract addresses", listed for Solana Devnet:
    // https://developers.circle.com/stablecoins/usdc-contract-addresses
    address: "4zMMC9srt5Ri5X14GAgXhaHii3GnPAEERYPJgZJDncDU",
    symbol: "USDC",
    name: "USD Coin",
    source: "https://developers.circle.com/stablecoins/usdc-contract-addresses",
  },
  {
    // Circle, "EURC contract addresses", testnet table, listed for Solana
    // Devnet: https://developers.circle.com/stablecoins/eurc-contract-addresses#testnet
    // Circle uses the same mint address for EURC on devnet and on mainnet.
    // That is deliberate, not a copying mistake.
    address: "HzwqbKZw8HxMN6bF2yFZNrht3c2iXXzpKcFu7uBEDKtr",
    symbol: "EURC",
    name: "Euro Coin",
    source: "https://developers.circle.com/stablecoins/eurc-contract-addresses#testnet",
  },
  {
    // Solana Foundation's PYUSD page, devnet mint, alongside the mainnet one:
    // https://solana.com/pyusd
    // and Paxos, "PYUSD on Test Networks":
    // https://docs.paxos.com/guides/stablecoin/pyusd/testnet
    address: "CXk2AMBfi3TwaEL2468s6zP8xq9NxTXjp9gjMgzeUynM",
    symbol: "PYUSD",
    name: "PayPal USD",
    source: "https://docs.paxos.com/guides/stablecoin/pyusd/testnet",
  },
  {
    // Paxos, "USDG on Test Networks", Solana Devnet:
    // https://docs.paxos.com/guides/stablecoin/usdg/testnet
    address: "4F6PM96JJxngmHnZLBh9n58RH4aTVNWvDs2nuwrT5BP7",
    symbol: "USDG",
    name: "Global Dollar",
    source: "https://docs.paxos.com/guides/stablecoin/usdg/testnet",
  },
  {
    // Paxos, "PAXG on Test Networks", Solana Devnet:
    // https://docs.paxos.com/guides/stablecoin/paxg/testnet
    address: "9LpnRN9CcSwj12xVT5r4HmqnE5N57FHfNJ1PiBYtEvYX",
    symbol: "PAXG",
    name: "PAX Gold",
    source: "https://docs.paxos.com/guides/stablecoin/paxg/testnet",
  },
  {
    // Jito, "Deployed Programs", JitoSOL mint on the devnet stake pool:
    // https://www.jito.network/docs/jitosol/jitosol-liquid-staking/security/deployed-programs/
    address: "J1tos8mqbhdGcF3pgj4PCKyVjzWSURcpLZU7pPGHxSYi",
    symbol: "JitoSOL",
    name: "Jito Staked SOL",
    source: "https://www.jito.network/docs/jitosol/jitosol-liquid-staking/security/deployed-programs/",
  },
];

const MAINNET: Array<PopularToken> = [
  SOL,
  {
    // Circle, "USDC contract addresses", listed for Solana mainnet:
    // https://developers.circle.com/stablecoins/usdc-contract-addresses
    address: "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v",
    symbol: "USDC",
    name: "USD Coin",
    source: "https://developers.circle.com/stablecoins/usdc-contract-addresses",
  },
  {
    // Circle, "EURC contract addresses", mainnet table:
    // https://developers.circle.com/stablecoins/eurc-contract-addresses
    // The same address as devnet EURC; Circle uses one address for both.
    address: "HzwqbKZw8HxMN6bF2yFZNrht3c2iXXzpKcFu7uBEDKtr",
    symbol: "EURC",
    name: "Euro Coin",
    source: "https://developers.circle.com/stablecoins/eurc-contract-addresses",
  },
  {
    // Solana Foundation's PYUSD page, mainnet mint: https://solana.com/pyusd
    address: "2b1kV6DkPAnxd5ixfnxCpjxmKwqjjaYmCZfHsFu24GXo",
    symbol: "PYUSD",
    name: "PayPal USD",
    source: "https://solana.com/pyusd",
  },
  {
    // Jito, "Deployed Programs", JitoSOL mint on the mainnet stake pool:
    // https://www.jito.network/docs/jitosol/jitosol-liquid-staking/security/deployed-programs/
    address: "J1toso1uCk3RLmjorhTtrVwY9HJ7X8V9yYac6Y7kGCPn",
    symbol: "JitoSOL",
    name: "Jito Staked SOL",
    source: "https://www.jito.network/docs/jitosol/jitosol-liquid-staking/security/deployed-programs/",
  },
  {
    // Coinbase's announcement of cbBTC on Solana, from its own account:
    // https://x.com/coinbase/status/1854570367773241556
    address: "cbbtcf3aa214zXHbiAZQwf4122FBYbraNdFqgw4iMij",
    symbol: "cbBTC",
    name: "Coinbase Wrapped BTC",
    source: "https://x.com/coinbase/status/1854570367773241556",
  },
];

export const POPULAR_TOKENS: Record<Cluster, Array<PopularToken>> = {
  localnet: [],
  devnet: DEVNET,
  testnet: [],
  mainnet: MAINNET,
};
