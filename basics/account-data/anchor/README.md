# Account Data (Anchor)

Store and retrieve arbitrary data in a Solana [account](https://solana.com/docs/terminology#account) owned by this [program](https://solana.com/docs/terminology#program).

See also: the [repository catalog](../../../README.md).

## Major concepts

- Account ownership and lamport rent
- Serializing and deserializing account data

## Setup

From this directory (`basics/account-data/anchor/`):

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
