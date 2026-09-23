# Solana Lending (Anchor)

> [!NOTE]
> This is the **Anchor v1** copy of this example, kept for programs staying on the
> Anchor v1 LTS line. Every `anchor` command on this page needs the v1 CLI:
> `avm install 1.2.0 && avm use 1.2.0`. The Anchor v2 version of this example is in
> [`../anchor`](../anchor/).

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
is `total_liquidity / total_shares`, where `total_liquidity = available_liquidity
+ current_debt` and `total_shares` is the share supply plus `MINIMUM_SHARES`.
`available_liquidity` (not the vault's raw token balance) is the source of truth,
so a token donated directly to the vault cannot inflate the rate.

That alone does not close the empty-pool inflation attack, because
`total_liquidity` also counts interest owed on borrows, and a supplier can
borrow. A lone supplier holding one share borrows a single base unit from their
own reserve; debt rounds up, so a second later it reads as two, and the share is
worth two units without anything having been minted. Deposits and redemptions
round in the pool's favor, so depositing the most that still mints one share
and redeeming it leaves the remainder with the only other share, the attacker's
own, and the price climbs about half again each round. After 49 rounds one share
is worth about 690 USDC, a 1,000 USDC deposit mints one share, and the attacker's
share redeems half the pool: 155 USDC of profit on a debt they repay. So the
first deposit mints 1:1 less `MINIMUM_SHARES` (1,000), which are never minted and
count as shares nobody holds in every conversion between shares and liquidity:
deposit, redeem, collateral valuation, collateral withdrawal and liquidation.
The attacker's one share is then 1 of 1,001, and what the rounding leaves behind
goes mostly to shares nobody can redeem.

### Interest: a kinked curve and a cumulative index

Each `refresh_reserve` advances `borrow_accumulation_factor` by
`(1 + rate_per_second * elapsed_seconds)`. `rate_per_second` comes from a kinked
utilization curve: linear from `min_borrow_rate_bps` to `optimal_borrow_rate_bps`
up to `optimal_utilization_bps`, then steeper to `max_borrow_rate_bps` at full
utilization. Each borrow stores its principal as **scaled debt** (principal ÷
index at borrow time), so every obligation's debt grows automatically as the
index advances: no per-obligation accrual loop.

Those curve parameters are annual, and the conversion to a per-second rate
divides by `SECONDS_PER_YEAR`. Elapsed time is the Clock's `unix_timestamp`
minus the reserve's `last_accrual_timestamp`, so a borrower pays the advertised
APR over a real year whatever the cluster's slot time is. Counting slots instead
would need a slots-per-year divisor, which is a guess at the slot length: the
network changes that length by feature gate, and does not deliver it exactly
between changes. The timestamp is written by each block's leader, but the
runtime bounds how far one block can move it, so an elapsed time is out by a
second or two at most, which is nothing against an annual rate. A timestamp at
or before the stored one accrues nothing. `update_reserve_config` accrues at the
old curve before storing a new one, so the seconds already elapsed are charged
at the rates that applied to them.

The reserve still records `last_update_slot`, for a different job: handlers that
read the reserve's value require the refresh to have run in the current slot.

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

`PriceFeed` mirrors an oracle price feed such as Pyth's: a signed mantissa, an
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
deterministic; in production a reserve points at a Pyth price feed and the
program reads its `PriceUpdateV2` account instead, as
[`basics/pyth`](../../../basics/pyth/) does, after checking the update's
`feed_id`: `price_mantissa` is `price_message.price`, `exponent` is
`price_message.exponent`, and `last_updated_slot` is `posted_slot`. It should
also reject results whose confidence interval is too wide.

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

- Rust and the Solana toolchain (`cargo-build-sbf`), Anchor 1.2.0, Solana 3.1.8.
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

Through a cumulative accumulation factor: `refresh_reserve` advances a per-reserve factor along a utilization-based rate curve, and each obligation stores the index value from its last interaction. The gap between the two is the interest owed, so no per-account accrual loop is needed. This is the same technique the most-used Solana lending protocols share.

### How are prices fed into the protocol?

The admin `set_price` instruction handler stands in for an oracle feed in this example. `refresh_obligation` re-values collateral and debt at those prices before any borrow, withdraw, or liquidation is allowed, and stale reserves or prices are rejected.

### How is this lending program tested and verified?

`anchor build` then `cargo test` runs LiteSVM integration tests covering interest accrual, borrowing at the LTV limit, liquidation after a price move, and the share-inflation guard. The money math also has [Kani](https://github.com/model-checking/kani) proofs in [`../kani-proofs/`](../kani-proofs/).
