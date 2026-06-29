# Token Swap (AMM) (Quasar)

Constant-product AMM: pools, liquidity, swaps with slippage guards.

See also: [Token Swap overview](../README.md) and the [repository catalog](../../../README.md).

## Major concepts

- Pool PDA and LP tokens
- See [finance/token-swap/README.md](../token-swap/README.md)

## Slippage protection

All three money-moving flows take a caller-supplied floor and revert with a
named `AmmError` if the floor is not met:

- `deposit_liquidity(amount_a, amount_b, minimum_lp_tokens_out)` treats
  `amount_a` / `amount_b` as upper bounds: one side is used in full and the
  other is scaled down to the current pool ratio (never up). If the LP tokens
  minted would fall below `minimum_lp_tokens_out`, the deposit reverts with
  `DepositBelowMinimum`.
- `withdraw_liquidity(amount, minimum_token_a_out, minimum_token_b_out)`
  reverts with `WithdrawalBelowMinimum` if either side of the proportional
  payout falls below its floor.
- `swap_tokens(input_is_token_a, input_amount, min_output_amount)` reverts
  with `SlippageExceeded` if the constant-product output falls below
  `min_output_amount`.

Requesting more than the caller's token balance fails fast with
`InsufficientBalance` rather than clamping, so the caller's slippage math
always refers to the amounts actually moved. Error codes live in
`src/error.rs` and start at 6000, matching the Anchor variant's offset.

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
