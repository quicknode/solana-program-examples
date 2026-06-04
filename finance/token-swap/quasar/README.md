# Token Swap (AMM) (Quasar)

Constant-product AMM: pools, liquidity, swaps with slippage guards.

See also: [Token Swap overview](../README.md) and the [repository catalog](../../../README.md).

## Major concepts

- Pool PDA and LP tokens
- See [finance/token-swap/README.md](../token-swap/README.md)

## Setup

From `finance/token-swap/quasar/`:

```bash
quasar build
```

Prerequisites: [Quasar](https://quasar-lang.com/docs) CLI and [Agave](https://docs.anza.xyz/) toolchain (see `Quasar.toml`).

## Testing

In-process tests via **Quasar SVM** (`quasar-svm` in `Quasar.toml`):

```bash
cargo test
```

Tests invoke instruction handlers and assert onchain state. No local validator.

## Usage

Read `src/` and `Quasar.toml`. Compare with the [Anchor](../anchor/) variant in the same example where present.
