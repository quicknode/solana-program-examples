# Checking Accounts (Anchor)

Validate that accounts passed into an [instruction](https://solana.com/docs/terminology#instruction) meet signer, owner, and address constraints before handler logic runs.

See also: [Checking Accounts overview](../README.md) and the [repository catalog](../../../README.md).

## Major concepts

- Account constraints in Anchor
- Signer vs mut vs address checks

## Setup

From this directory (`basics/checking-accounts/anchor/`):

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

This runs `cargo test` as configured in `Anchor.toml`.

## Usage

Read the program `programs/` source and `Anchor.toml` for deployed program IDs. For deployment, use `anchor build && anchor deploy` against your target cluster.
