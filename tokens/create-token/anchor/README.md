# Create Token (Anchor)

Create a token mint with metadata (symbol, name, uri) via the Classic Token and Metaplex Token Metadata programs.

See also: [Create Token overview](../README.md) and the [repository catalog](../../../README.md).

## Major concepts

- Mint and metadata accounts
- See [tokens/create-token/README.md](../README.md)

## Setup

From this directory (`tokens/create-token/anchor/`):

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

(`anchor test` runs the command configured in `Anchor.toml` `[scripts] test`.) Tests call instruction handlers and check onchain state.

## Usage

Read the program `programs/` source and `Anchor.toml` for deployed program IDs. For deployment, use `anchor build && anchor deploy` against your target cluster.
