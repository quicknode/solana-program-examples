# Realloc (Anchor)

Grow or shrink account data when variable-length storage is required.

See also: [Realloc overview](../README.md) and the [repository catalog](../../../README.md).

## Major concepts

- Account reallocation
- Rent adjustment on size change

## Setup

From this directory (`basics/realloc/anchor/`):

```bash
pnpm install
anchor build
```

Prerequisites: [Agave](https://docs.anza.xyz/) CLI (version in `Anchor.toml` `[toolchain]`), [Anchor](https://www.anchor-lang.com/docs), and `pnpm`.

## Testing

Tests run in-process with [LiteSVM](https://www.anchor-lang.com/docs/testing/litesvm). No local validator.

```bash
anchor test
```

(`anchor test` runs the command configured in `Anchor.toml` `[scripts] test`.)

## Usage

Read the program `programs/` source and `Anchor.toml` for deployed program IDs. For deployment, use `anchor build && anchor deploy` against your target cluster.
