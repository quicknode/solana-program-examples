# Transfer Hook - Allow/Block List (Anchor)

Restrict transfers using an onchain allow/block list enforced by a transfer hook program.

See also: [Allow Block List Token overview](../README.md) and the [repository catalog](../../../README.md).

## Major concepts

- Transfer hook
- Extra account metas
- List authority

## Setup

From this directory (`tokens/token-extensions/transfer-hook/allow-block-list-token/anchor/`):

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
