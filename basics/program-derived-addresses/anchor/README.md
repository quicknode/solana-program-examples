# Program Derived Addresses (Anchor)

Derive and use PDAs as deterministic account addresses for program-owned state.

See also: the [repository catalog](../../../README.md).

## Major concepts

- PDA derivation with seeds
- Storing state at a PDA

## Setup

From this directory (`basics/program-derived-addresses/anchor/`):

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
