# Transfer Hook - Hello World (Anchor)

Minimal transfer hook that runs custom logic on every token transfer.

See also: the [repository catalog](../../../../../README.md).

## Major concepts

- Transfer hook program
- Extra account meta list

## Setup

From this directory (`tokens/token-extensions/transfer-hook/hello-world/anchor/`):

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
