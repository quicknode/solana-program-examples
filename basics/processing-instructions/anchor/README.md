# Processing Instructions (Anchor)

Pass arguments into an [instruction handler](https://solana.com/docs/terminology#instruction-handler) and use them in program logic.

See also: [Processing Instructions overview](../README.md) and the [repository catalog](../../../README.md).

## Major concepts

- Instruction data deserialization
- Handler parameters

## Setup

From this directory (`basics/processing-instructions/anchor/`):

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
