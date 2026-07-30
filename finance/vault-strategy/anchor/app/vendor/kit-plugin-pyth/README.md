# kit-plugin-pyth

Pyth Network oracle plugin for [Solana Kit](https://www.solanakit.com). Adds a `pyth`
namespace to the client for reading Pyth price feeds via the Hermes API and onchain price
accounts, and for posting pull-oracle price updates.

Layers on the [`kite()`](../kit-plugin-kite) capability, so apply
[`kite()`](../kit-plugin-kite) first.

## Installation

```bash
npm install kit-plugin-pyth kit-plugin-kite solana-kite @solana/kit
```

## Quick Start

```typescript
import { createClient } from "@solana/kit";
import { kite } from "kit-plugin-kite";
import { pyth } from "kit-plugin-pyth";
import { PYTH_FEED_IDS } from "kit-plugin-pyth";

const connection = createClient()
  .use(kite({ clusterNameOrURL: "mainnet" }))
  .use(pyth());

// Latest SOL/USD price feed (spot price + EMA) from Hermes
const priceFeed = await connection.pyth.getPythPriceFeed(PYTH_FEED_IDS.SOL_USD);
console.log(`SOL/USD: $${priceFeed.price.price}`);
```

## Methods

All methods live under the `pyth` namespace on the client.

- `getPythPriceFeed(feedId)` fetches the latest `PythPriceFeed` (spot + EMA price) for one feed from Hermes.
- `getPythPriceFeeds(feedIds)` fetches multiple feeds in a single request.
- `getPythOnchainPrice(priceAccountAddress)` reads a price directly from an onchain Pyth price account. Live mainnet prices come from the pull oracle; this decodes legacy push-oracle account layouts.
- `isPythPriceStale(feedId, maxAgeSeconds)` returns `true` if the feed's last publish time exceeds `maxAgeSeconds`.
- `searchPythFeeds(query, assetType?)` searches Pyth's feed catalogue by name or symbol.
- `watchPythPriceFeed(feedId, callback, intervalMs?)` polls a feed and invokes `callback` on each update. Returns a stop function.
- `postPythPriceUpdate(feedId, payer)` posts a single pull-oracle price update onchain and returns the temporary `PriceUpdateV2` account address.
- `postPythPriceUpdates(feedIds, payer)` posts multiple price updates (one transaction per feed).
- `reclaimPythPriceUpdateRent(priceUpdateAccount, payer)` closes a posted price-update account and recovers its rent.

## Configuration

- `hermesUrl` (string, default `https://hermes.pyth.network`): base URL for the Pyth Hermes API.

## Other exports

- `PythClient` is the class behind the `pyth` namespace, usable directly without the plugin wrapper.
- `parsePythPriceAccountData(data)` decodes raw price-account bytes; returns `null` for data that is not a Pyth price account.
- `PYTH_FEED_IDS` holds well-known feed IDs (SOL/USD, BTC/USD, ETH/USD, USDC/USD, USDT/USD, BNB/USD).
- `HERMES_URL`, `PYTH_RECEIVER_PROGRAM_ID`, `WORMHOLE_PROGRAM_ID`, and the `PYTH_*` layout constants back the client's decoding and transaction building; the legacy `SOL_USD_PRICE_ACCOUNT`, `BTC_USD_PRICE_ACCOUNT`, and `ETH_USD_PRICE_ACCOUNT` push-oracle addresses are decoding references for tests.

## Testing

```bash
npm run test:ci        # offline tests (price account decoding), what CI runs
npm run test:litesvm   # decode a synthetic price account over LiteSVM
npm test               # full suite, including Hermes network tests
```

## License

MIT
