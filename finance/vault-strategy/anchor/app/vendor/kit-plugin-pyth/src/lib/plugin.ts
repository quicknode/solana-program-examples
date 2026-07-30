import { extendClient } from "@solana/kit";
import type { Connection } from "solana-kite";
import { PythClient } from "./pyth-client.js";
import type { PythConfig, PythMethods } from "./types.js";

/**
 * Solana Kit plugin that adds Pyth Network oracle helpers (under `client.pyth`) to a client.
 *
 * Requires the kite() capability (apply `kite()` from `kit-plugin-kite` first), as the
 * Pyth client uses the connection's RPC and transaction helpers.
 *
 * @example
 * ```typescript
 * import { createClient } from "@solana/kit";
 * import { kite } from "kit-plugin-kite";
 * import { pyth } from "kit-plugin-pyth";
 *
 * const client = createClient()
 *   .use(kite({ clusterNameOrURL: "mainnet" }))
 *   .use(pyth());
 *
 * const feed = await client.pyth.getPythPriceFeed("...");
 * ```
 */
export function pyth(config: PythConfig = {}) {
  return <T extends Connection>(connection: T) => {
    const client = new PythClient(connection, config);

    const pythMethods: PythMethods = {
      getPythPriceFeed: client.getPythPriceFeed.bind(client),
      getPythPriceFeeds: client.getPythPriceFeeds.bind(client),
      getPythOnchainPrice: client.getPythOnchainPrice.bind(client),
      getPythPriceUpdateV2: client.getPythPriceUpdateV2.bind(client),
      isPythPriceStale: client.isPythPriceStale.bind(client),
      searchPythFeeds: client.searchPythFeeds.bind(client),
      watchPythPriceFeed: client.watchPythPriceFeed.bind(client),
      postPythPriceUpdate: client.postPythPriceUpdate.bind(client),
      postPythPriceUpdates: client.postPythPriceUpdates.bind(client),
      reclaimPythPriceUpdateRent: client.reclaimPythPriceUpdateRent.bind(client),
    };

    return extendClient(connection, { pyth: pythMethods });
  };
}
