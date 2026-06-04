# Pyth Price Feeds (Anchor)

Read a [Pyth](https://pyth.network/) price feed account and log price, confidence, and exponent.

See also: [Pyth overview](../README.md) and the [repository catalog](../../../README.md).

## Major concepts

- Oracle price accounts
- Consuming external onchain data in a program

## Setup

From this directory (`basics/pyth/anchor/`):

```bash
pnpm install
anchor build
```

Prerequisites: [Agave](https://docs.anza.xyz/) CLI (version in `Anchor.toml` `[toolchain]`), [Anchor](https://www.anchor-lang.com/docs), and `pnpm`.

## Testing

Tests run in-process with [LiteSVM](https://www.anchor-lang.com/docs/testing/litesvm). No local validator.

```bash
pnpm test
```

This runs `cargo test` as configured in `Anchor.toml`. Tests call instruction handlers and check onchain state.

## Usage

Read the program `programs/` source and `Anchor.toml` for deployed program IDs. For deployment, use `anchor build && anchor deploy` against your target cluster.
