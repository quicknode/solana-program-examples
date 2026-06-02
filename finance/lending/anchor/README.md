# Lending

A Kamino/Solend-style borrow/lend program: suppliers earn interest on deposits,
borrowers post collateral and draw other assets against it, and liquidators keep
the market solvent. It demonstrates the techniques the most-used Solana lending
protocols share — share-token deposit accounting, a utilization-based interest
index, oracle-priced obligation health, and close-factor-capped liquidation.

## Purpose

Lending markets let one set of users supply liquidity to earn yield while another
set borrows it against collateral. This program implements that end to end:

- **Suppliers** deposit a token and receive **share tokens** representing their
  slice of the pool. The share-to-liquidity exchange rate rises as borrowers pay
  interest, so redeeming later returns more than was deposited.
- **Borrowers** post their share tokens as collateral in an obligation and borrow
  a different token, up to a loan-to-value limit.
- **Liquidators** repay part of an unhealthy obligation's debt and seize its
  collateral at a discount, pulling the position back to solvency.

Concrete directional example (a short): supply USDC and post the USDC share
tokens as collateral, borrow NVDAx, and sell it. You are **long your collateral
(USDC) and short the borrowed asset (NVDAx)**. While the loan is open you pay a
variable borrow rate that tracks pool utilization. Buy NVDAx back later, call
`repay_obligation_liquidity`, then `withdraw_obligation_collateral` and
`redeem_reserve_collateral` to exit. If NVDAx instead rises far enough, your debt
crosses the liquidation threshold and a liquidator can close part of the position.

## Major Concepts

### Accounts

- **`LendingMarket`** — top-level config (owner, quote-currency mint). PDA seeds
  `["lending_market", owner]`.
- **`Reserve`** — one per asset. Owns a program-controlled liquidity vault and a
  share-token mint, and stores the interest-rate config, the cumulative borrow-
  rate index, available liquidity, and scaled total debt. PDA seeds
  `["reserve", market, liquidity_mint]`.
- **`Obligation`** — one per borrower per market: the share-token collateral
  posted and the liquidity borrowed, with cached quote-currency valuations. PDA
  seeds `["obligation", market, owner]`.
- **`PriceFeed`** — a price for one token (see Oracle below).

### Share tokens (the deposit claim)

Supplying liquidity mints share tokens; redeeming burns them. The exchange rate
is `total_liquidity / share_supply`, where `total_liquidity = available_liquidity
+ current_debt`. `available_liquidity` (not the vault's raw token balance) is the
source of truth, so a token donated directly to the vault cannot inflate the rate
— closing the classic empty-pool inflation attack. The first deposit mints 1:1.

### Interest: a kinked curve and a cumulative index

Each `refresh_reserve` advances `cumulative_borrow_rate_index` by
`(1 + rate_per_slot * elapsed_slots)`. `rate_per_slot` comes from a kinked
utilization curve — linear from `min_borrow_rate_bps` to `optimal_borrow_rate_bps`
up to `optimal_utilization_bps`, then steeper to `max_borrow_rate_bps` at full
utilization. Each borrow stores its principal as **scaled debt** (principal ÷
index at borrow time), so every obligation's debt grows automatically as the
index advances — no per-obligation accrual loop.

### Obligation health

`refresh_obligation` recomputes, from the refreshed reserves and their prices:
`borrowed_value`, `allowed_borrow_value` (Σ collateral value × `loan_to_value_bps`)
and `unhealthy_borrow_value` (Σ collateral value × `liquidation_threshold_bps`).
Borrowing and withdrawing are gated by `allowed_borrow_value`; an obligation is
liquidatable once `borrowed_value > unhealthy_borrow_value`. Collateral is valued
rounding down and debt rounding up, so health is always judged conservatively.

### Fixed-point math

All money math is integer-only `u128` — no floats, no fixed-point crates. Ratios
(rates, the index, the exchange rate, obligation values) are scaled by
`FIXED_POINT_SCALE` (10^18). Every conversion rounds in the protocol's favour
(user output floored, debt ceiled), so dust cannot be extracted by repeated
round-trips.

### Oracle

`PriceFeed` mirrors a Switchboard On-Demand pull feed: a signed mantissa, an
exponent (`price = mantissa * 10^exponent`), and the slot the price was written.
Freshness is checked in **slots** (`MAX_PRICE_STALENESS_SLOTS`), not wall-clock
time. The `set_price` handler writes the feed directly so the LiteSVM tests are
deterministic; in production a reserve points at the real Switchboard feed and the
program decodes `PullFeedAccountData` (`price_mantissa = current_result.value`,
`exponent = -18`, `last_updated_slot = current_result.slot`) instead. Switchboard
is used rather than Pyth here for its lower compute cost.

### Custody

Supplied liquidity sits in program-owned vault PDAs, and posted collateral sits in
per-obligation vault PDAs whose authority is the obligation PDA. The market owner
can update reserve risk parameters (`update_reserve_config`) but has no path to
move user funds — there is no admin withdrawal or escape hatch.

### Instruction handlers

Admin: `init_lending_market`, `init_reserve`, `update_reserve_config`, `set_price`.
Supply side: `refresh_reserve`, `deposit_reserve_liquidity`,
`redeem_reserve_collateral`. Borrow side: `init_obligation`, `refresh_obligation`,
`deposit_obligation_collateral`, `withdraw_obligation_collateral`,
`borrow_obligation_liquidity`, `repay_obligation_liquidity`, `liquidate_obligation`.

Value-dependent handlers require the reserves and the obligation to have been
refreshed in the same transaction, so a typical action transaction is
`[refresh_reserve …, refresh_obligation, <action>]`.

## Setup

- Rust and the Solana toolchain (`cargo-build-sbf`), Anchor 1.0.x, Solana 3.1.8.
- This program has no client/JavaScript code; tests are Rust + LiteSVM.

## Testing

```sh
anchor build   # or: cargo build-sbf — produces target/deploy/lending.so
anchor test    # or: cargo test     — runs the LiteSVM integration tests
```

`anchor build` (or `cargo build-sbf`) must run first: the tests load the compiled
`target/deploy/lending.so` via `include_bytes!`. The suite covers the
non-happy-path branches — interest accrual, borrowing at the LTV limit, stale
reserve/price rejection, liquidation of an unhealthy obligation after a price
move, the share-inflation guard, and rounding edges.
