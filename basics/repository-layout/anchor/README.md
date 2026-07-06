# Repository Layout (Anchor)

Organize a larger program across modules (state, instructions, errors) instead of a single file.

See also: [Repository Layout overview](../README.md) and the [repository catalog](../../../README.md).

## Major concepts

- Multi-file Anchor layout
- Separating concerns by module

## Setup

From this directory (`basics/repository-layout/anchor/`):

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
