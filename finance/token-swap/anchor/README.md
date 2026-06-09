# Token Swap (AMM) (Anchor)

Constant-product AMM pool: create pools, deposit liquidity, swap with slippage guards, and withdraw.

See also: [Token Swap overview](../README.md) and the [repository catalog](../../../README.md).

## Major concepts

- Liquidity pool PDA
- LP tokens and swap invariant
- See [finance/token-swap/README.md](../README.md) for the full walkthrough

## Setup

From this directory (`finance/token-swap/anchor/`):

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
