# Hello Solana (Anchor)

Minimal program that logs a greeting. Introduces transactions, instructions, and program entrypoints.

See also: [Hello Solana overview](../README.md) and the [repository catalog](../../../README.md).

## Major concepts

- Program entrypoint and instruction data
- Logging from onchain code

## Setup

From this directory (`basics/hello-solana/anchor/`):

```bash
anchor build
```

Prerequisites: the [Agave](https://docs.anza.xyz/) toolchain and the [Anchor](https://www.anchor-lang.com/docs) CLI.

## Testing

Tests run in-process with [LiteSVM](https://www.anchor-lang.com/docs/testing/litesvm). No local validator.

```bash
anchor test
```

(`anchor test` runs the command configured in `Anchor.toml` `[scripts] test`.) Tests call instruction handlers and check onchain state.

## Usage

Read the program `programs/` source and `Anchor.toml` for deployed program IDs. For deployment, use `anchor build && anchor deploy` against your target cluster.
