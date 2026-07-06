# Create Account (Anchor)

Create new onchain accounts and fund them for rent exemption using the System Program.

See also: [Create Account overview](../README.md) and the [repository catalog](../../../README.md).

## Major concepts

- Account creation CPI
- Rent-exempt lamport funding

## Setup

From this directory (`basics/create-account/anchor/`):

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
