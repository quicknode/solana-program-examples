# Pyth Price Feeds (Anchor)

> [!NOTE]
> This is the **Anchor v2** copy of this example. Every `anchor` command on this page
> needs the v2 CLI: `cargo install anchor-cli --version 2.0.0-rc.1 --locked` (avm has
> no prebuilt binary for this pre-release). The Anchor v1 version of this example is in
> [`../anchor-v1`](../anchor-v1/).

Read a [Pyth](https://pyth.network/) price feed account and log price, confidence, and exponent.

See also: [Pyth overview](../README.md) and the [repository catalog](../../../README.md).

> [!NOTE]
> **This example vendors the `PriceUpdateV2` account type rather than importing `pyth-solana-receiver-sdk`.**
>
> The SDK's current release (2.0.0, checked August 2026) builds against `anchor-lang` 1.0.2, and this repository is on 2.0.0-rc.1, whose account wrappers are a different set of types. Importing the SDK's `PriceUpdateV2` would pull a second `anchor-lang` into the graph.
>
> `programs/pythexample/src/lib.rs` mirrors the onchain layout instead: same fields in the same order, same 8-byte discriminator, owned by the Pyth Receiver program, so accounts written by Pyth deserialize unchanged. Import the SDK type once a release targeting `anchor-lang` 2.x ships.

## Major concepts

- Oracle price accounts
- Consuming external onchain data in a program
- Oracle account validation: `Account<PriceUpdateV2>` enforces that the price account is owned by the Pyth Receiver program (`rec5EKMGg6MxZYaMdyBfgwp4d5rB9T1VQH5pJv5LtFJ`)
- Price freshness: `read_price` rejects updates older than `MAXIMUM_PRICE_AGE_SECONDS` (compared against `publish_time`, a unix timestamp in seconds, mirroring the SDK's `get_price_no_older_than`)

## Setup

From this directory (`basics/pyth/anchor/`):

```bash
anchor build
```

Prerequisites: [Agave](https://docs.anza.xyz/) CLI (version in `Anchor.toml` `[toolchain]`), [Anchor](https://www.anchor-lang.com/docs).

## Testing

Tests run in-process with [LiteSVM](https://www.anchor-lang.com/docs/testing/litesvm). No local validator.

```bash
anchor test
```

This runs `cargo test` as configured in `Anchor.toml`. Tests call instruction handlers and check onchain state.

## Usage

Read the program `programs/` source and `Anchor.toml` for deployed program IDs. For deployment, use `anchor build && anchor deploy` against your target cluster.
