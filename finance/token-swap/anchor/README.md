# Solana Token Swap AMM (Anchor)

A constant-product AMM (automated market maker) on Solana: create pools, deposit liquidity, swap with slippage guards, and withdraw. This is the exchange design behind Solana venues like Raydium and Orca.

See also: [Token Swap overview](../README.md) and the [repository catalog](../../../README.md).

## Major concepts

- Liquidity pool PDA
- LP tokens and swap invariant
- See [finance/token-swap/README.md](../README.md) for the full walkthrough

## Setup

From this directory (`finance/token-swap/anchor/`):

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

## FAQ

### How does an AMM work on Solana?

An automated market maker replaces the order book with a liquidity pool: anyone can create a pool with `create_pool`, fund it with `deposit_liquidity`, and trade against it with `swap_tokens`. Prices come from the constant-product invariant on the pool's balances, and liquidity providers earn a share of trading fees. Solana exchanges like Raydium and Orca use this design.

### How is slippage handled?

`swap_tokens` takes a `min_output_amount` guard: if the pool's balances move so the trade would return less than that minimum, the transaction fails instead of filling at a worse price.

### Where is the full walkthrough for this example?

The example-level [Token Swap overview](../README.md) covers the pool math, LP tokens, and lifecycle; this page covers the Anchor build and test commands. The money math has [Kani](https://github.com/model-checking/kani) proofs in [`../kani-proofs/`](../kani-proofs/).
