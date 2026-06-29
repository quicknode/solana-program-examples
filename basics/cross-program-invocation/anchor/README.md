# Cross-Program Invocation (Anchor)

Invoke another program from this one (hand program calls lever program) using CPI.

See also: [Cross Program Invocation overview](../README.md) and the [repository catalog](../../../README.md).

## Major concepts

- Cross-program invocation (CPI)
- Composing multiple programs in one transaction

## Setup

From this directory (`basics/cross-program-invocation/anchor/`):

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
