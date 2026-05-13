# Pyth Price Feeds

[Pyth](https://pyth.network/) is an oracle that publishes low-latency market data from institutional sources onchain. You can use it to read real-world asset prices from Solana programs.

Each asset's price lives in its own Solana account — a **price feed**.

For example, the SOL/USD price feed on mainnet lives at `H6ARHf6YXhGYeQfUzQNGk6rDNnLBQKrenN712K4AQJEG`.

You can find more feeds in the [Pyth feed list](https://pyth.network/price-feeds?cluster=mainnet-beta).

To use a feed, pass its account into your instruction handler's context, then read the account's data. A feed contains:

- A price.
- A confidence interval.
- An exponent.

See the [Pyth Solana docs](https://docs.pyth.network/price-feeds/core/use-real-time-data/pull-integration/solana) for the full data layout and integration guide.
