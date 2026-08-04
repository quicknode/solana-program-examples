# Solana Lending (Anchor)

A Kamino/Solend-style borrow/lend program on Solana: suppliers earn interest on deposits,
borrowers post collateral and draw other assets against it, and liquidators keep
the market solvent. It demonstrates the techniques the most-used Solana lending
protocols share: share-token deposit accounting, a utilization-based interest
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

- **`LendingMarket`**: top-level config (owner, quote-currency mint). PDA seeds
  `["lending_market", market_id]`, where `market_id` is a `u64` index. Seeding by
  an index alone (owner is stored as a field for authorization, not baked into the
  address) lets one owner run several independent, risk-isolated markets (their
  market 0, 1, 2 …) with no cross-owner collisions and no individual's key in a
  shared struct's address.
- **`Reserve`**: one per asset. Owns a program-controlled liquidity vault and a
  share-token mint, and stores the interest-rate config, the cumulative borrow-
  rate index, available liquidity, and scaled total debt. PDA seeds
  `["reserve", market, liquidity_mint]`.
- **`Obligation`**: one per borrower per market: the share-token collateral
  posted and the liquidity borrowed, with cached quote-currency valuations. PDA
  seeds `["obligation", market, owner]`.
- **`PriceFeed`**: a price for one token (see Oracle below).

### Share tokens (the deposit claim)

Supplying liquidity mints share tokens; redeeming burns them. The exchange rate
is `total_liquidity / share_supply`, where `total_liquidity = available_liquidity
+ current_debt`. `available_liquidity` (not the vault's raw token balance) is the
source of truth, so a token donated directly to the vault cannot inflate the rate,
closing the classic empty-pool inflation attack. The first deposit mints 1:1.

### Interest: a kinked curve and a cumulative index

Each `refresh_reserve` advances `cumulative_borrow_rate_index` by
`(1 + rate_per_slot * elapsed_slots)`. `rate_per_slot` comes from a kinked
utilization curve: linear from `min_borrow_rate_bps` to `optimal_borrow_rate_bps`
up to `optimal_utilization_bps`, then steeper to `max_borrow_rate_bps` at full
utilization. Each borrow stores its principal as **scaled debt** (principal ÷
index at borrow time), so every obligation's debt grows automatically as the
index advances: no per-obligation accrual loop.

### Protocol fees (how the market earns)

Borrowers owe the full interest, but suppliers don't receive all of it. On each
accrual the reserve keeps `config.reserve_factor_bps` of the freshly accrued
interest in `accumulated_protocol_fees`; only the remainder lifts the supplier
exchange rate. Those fees are carved out of `total_liquidity`, so they never
count as a supplier claim, and the market owner withdraws them with
**`collect_protocol_fees`** (paid out of the reserve's available liquidity).
This spread between the borrow rate and the supply rate is the protocol's revenue.

### Obligation health

`refresh_obligation` recomputes, from the refreshed reserves and their prices:
`borrowed_value`, `allowed_borrow_value` (Σ collateral value × `loan_to_value_bps`)
and `unhealthy_borrow_value` (Σ collateral value × `liquidation_threshold_bps`).
Borrowing and withdrawing are gated by `allowed_borrow_value`; an obligation is
liquidatable once `borrowed_value > unhealthy_borrow_value`. Collateral is valued
rounding down and debt rounding up, so health is always judged conservatively.

Every handler that pairs an obligation with a reserve requires both to belong to
the same `LendingMarket` (`MarketMismatch` otherwise), so each market is an
isolation boundary: positions in one market can never be valued or settled
against reserves of another.

In a liquidation, the close factor (how much of the borrow one call may repay)
comes from the **repay reserve**, because it is a property of the debt being
closed; the liquidation bonus comes from the **collateral reserve**, because it
prices the collateral being seized. A repayment whose seizure would exceed the
posted collateral fails with `LiquidationTooLarge` rather than silently seizing
less, which would make the liquidator overpay.

### Fixed-point math

All money math is integer-only `u128`: no floats, no fixed-point crates. Ratios
(rates, the index, the exchange rate, obligation values) are scaled by
`FIXED_POINT_SCALE` (10^18). Every conversion rounds in the protocol's favour
(user output floored, debt ceiled), so dust cannot be extracted by repeated
round-trips.

### Oracle

`PriceFeed` mirrors a Switchboard On-Demand pull feed: a signed mantissa, an
exponent (`price = mantissa * 10^exponent`), and the slot the price was written.
Freshness is checked in **slots** (`MAX_PRICE_STALENESS_SLOTS`), not wall-clock
time, plus one check slots alone cannot make: a cluster restart passes hours of
wall-clock time in zero slots, so `price_scaled` also rejects any price stamped
at or before the `LastRestartSlot` sysvar's slot, pausing valuation until the
publisher posts again. The feed PDA is seeded by `[b"price_feed", market, mint]` (scoped to a
market, not to any individual) and only that market's `owner` may write it
(`set_price` checks `has_one = owner`). So prices can't be squatted, a reserve
trusts exactly its own market's feed for the mint, and isolated markets can
price the same asset independently.

The `set_price` handler writes the feed directly so the LiteSVM tests are
deterministic; in production a reserve points at the real Switchboard feed and the
program decodes `PullFeedAccountData` (`price_mantissa = current_result.value`,
`exponent = -18`, `last_updated_slot = current_result.slot`) instead, and should
also reject results whose confidence interval is too wide. Switchboard is used
rather than Pyth here for its lower compute cost.

### Custody

Supplied liquidity sits in program-owned vault PDAs, and posted collateral sits in
per-obligation vault PDAs whose authority is the obligation PDA. The market owner
can update reserve risk parameters (`update_reserve_config`) and withdraw the
protocol's earned fees (`collect_protocol_fees`), but has no path to a supplier's
deposits or a borrower's collateral: there is no admin escape hatch over user funds.

### Known limits

- **Tokens with transfer fees are not supported.** The program uses
  `token_interface`, so Token Extensions mints are accepted, but a transfer-fee
  extension would make the vault receive less than the recorded deposit and the
  accounting would overstate `available_liquidity`. Production protocols
  whitelist mints; a market owner here must only create reserves for tokens
  without transfer fees.
- **Reserve config changes act immediately.** Lowering a reserve's
  `liquidation_threshold_bps` can make existing obligations liquidatable at
  once; production governance phases such changes in.
- This is an example. Deploying any program that custodies funds calls for a
  professional security audit first.

### Instruction handlers

Admin: `initialize_lending_market`, `initialize_reserve`, `update_reserve_config`, `set_price`,
`collect_protocol_fees`.
Supply side: `refresh_reserve`, `deposit_reserve_liquidity`,
`redeem_reserve_collateral`. Borrow side: `initialize_obligation`, `refresh_obligation`,
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
anchor build   # or: cargo build-sbf - produces target/deploy/lending.so
anchor test    # or: cargo test     - runs the LiteSVM integration tests
```

`anchor build` (or `cargo build-sbf`) must run first: the tests load the compiled
`target/deploy/lending.so` via `include_bytes!`. The suite covers the
non-happy-path branches: interest accrual, borrowing at the LTV limit, stale
reserve/price rejection, liquidation of an unhealthy obligation after a price
move, the share-inflation guard, and rounding edges.

## FAQ

### How does a lending protocol work on Solana?

Suppliers deposit a token with `deposit_reserve_liquidity` and receive share tokens that grow in value as borrowers pay interest. Borrowers post those shares as collateral (`deposit_obligation_collateral`) and draw a different token with `borrow_obligation_liquidity`, up to a loan-to-value limit. When a position's collateral no longer covers its debt, anyone can call `liquidate_obligation` to repay part of the debt in exchange for discounted collateral.

### How does interest accrue without looping over every account?

Through a cumulative interest index: `refresh_reserve` advances a per-reserve index along a utilization-based rate curve, and each obligation stores the index value from its last interaction. The gap between the two is the interest owed, so no per-account accrual loop is needed. This is the same technique the most-used Solana lending protocols share.

### How are prices fed into the protocol?

The admin `set_price` instruction handler stands in for an oracle feed in this example. `refresh_obligation` re-values collateral and debt at those prices before any borrow, withdraw, or liquidation is allowed, and stale reserves or prices are rejected.

### How is this lending program tested and verified?

`anchor build` then `cargo test` runs LiteSVM integration tests covering interest accrual, borrowing at the LTV limit, liquidation after a price move, and the share-inflation guard. The money math also has [Kani](https://github.com/model-checking/kani) proofs in [`../kani-proofs/`](../kani-proofs/).
