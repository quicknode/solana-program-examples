# Token Extensions - Basics (Anchor)

Create mints, mint tokens, and transfer using the [Token Extensions Program](https://solana.com/docs/terminology#token-extensions-program).

See also: the [repository catalog](../../../../README.md).

## Major concepts

- Extension-enabled mints
- Token Extensions CPI

## Setup

From this directory (`tokens/token-extensions/basics/anchor/`):

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
