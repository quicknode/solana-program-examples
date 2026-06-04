# Rent (Anchor)

Calculate account size and the minimum rent-exempt [lamport](https://solana.com/docs/terminology#lamport) balance.

See also: [Rent overview](../README.md) and the [repository catalog](../../../README.md).

## Major concepts

- Rent-exempt threshold
- Account space planning

## Setup

From this directory (`basics/rent/anchor/`):

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
