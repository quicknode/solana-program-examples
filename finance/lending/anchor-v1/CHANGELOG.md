# Changelog

## 2026-09-23

Lock a minimum number of reserve shares. The first deposit now mints
`deposit - MINIMUM_SHARES` (1,000) shares, and every conversion between shares
and liquidity (deposit, redeem, collateral valuation in `refresh_obligation`,
`withdraw_obligation_collateral` and liquidation) divides by the share supply
plus that minimum, through the new `Reserve::total_shares`. The withheld shares
belong to nobody, so their slice of the pool stays locked. A first deposit of
`MINIMUM_SHARES` or less fails with `DepositTooSmall`, and a reserve whose
suppliers have all left prices the next deposit against the locked slice
rather than bootstrapping it.

Pricing shares against tracked `total_liquidity` stopped a vault donation, but
`total_liquidity` includes interest owed on borrows. A lone supplier with one
share could borrow a single base unit from their own reserve, let one second of
interest round the debt up to two, and then ratchet the share price up about
half again per round by depositing the most that still minted one share and
redeeming it, leaving the rounding with their own share. On the default rate
curve, with no bad debt and the loan repaid, 49 rounds made one share worth
about 690 USDC, and a 1,000 USDC deposit made after that minted one share, so
the attacker redeemed half the pool and kept 155 USDC of it. Tested by
`inflating_shares_through_own_borrow_does_not_pay`, which fails against the
previous program, and by `first_deposit_must_exceed_the_minimum` and
`sole_supplier_leaves_the_minimum_behind`. The test harness's `add_reserve` now
opens each reserve with a deposit from the market owner; `add_empty_reserve`
leaves it empty.

## 2026-09-22

Accrue interest by the wall clock instead of by slots. The reserve's
`slots_per_year` config field was a guess at the cluster's slot length, and the
test default of 78,840,000 (a 400 ms slot) charged twice the advertised APR once
the network moved to 200 ms slots. Interest now accrues for the seconds between
the Clock's `unix_timestamp` and the new `Reserve::last_accrual_timestamp`, at
the APR divided by `SECONDS_PER_YEAR`; `slots_per_year` is gone from
`ReserveConfig`, and `current_borrow_rate_per_slot` is now
`current_borrow_rate_per_second`. A timestamp at or before the stored one
accrues nothing. `last_update_slot` stays, as the same-slot refresh check.
`update_reserve_config` now accrues at the old curve before storing the new one.
Tested by `interest_accrues_by_seconds_not_slots`,
`a_timestamp_behind_the_last_accrual_charges_nothing` and
`a_config_update_accrues_at_the_old_rates_first`, which replace
`slots_per_year_scales_the_per_slot_rate` and `rejects_zero_slots_per_year`.

## 2026-08-14

Move slots-per-year out of the code and into the reserve config. Turning an APR
into the per-slot rate interest accrues at needs a slots-per-year divisor, and
that divisor is the cluster's slot time in disguise. It was a `SLOTS_PER_YEAR`
constant fixed at a 400ms slot, so a protocol change to the slot time would have
raised the wall-clock rate every borrower pays with no code change and nothing
to show for it. `ReserveConfig` now carries `slots_per_year`, `validate()`
rejects zero, and the market owner retunes it with `update_reserve_config`.
Tested by `slots_per_year_scales_the_per_slot_rate` and
`rejects_zero_slots_per_year`.

## 2026-08-04

Reject oracle prices from before a cluster restart. A halt stops the slot
count but not the wall clock, so after a restart a feed can look fresh in
slots while its price is hours old. `price_scaled` now also requires the
feed's slot to be after the `LastRestartSlot` sysvar's slot
(`PricePredatesRestart`), pausing valuation until the publisher posts again.
Tested by `borrow_with_price_from_before_a_restart_is_rejected`.

## 0.1.0

Initial lending program: a Kamino/Solend-style borrow/lend market.

- Lending market, per-asset reserves with a program-owned liquidity vault and a
  share-token mint, and per-borrower obligations.
- Share-token deposit accounting with an exchange rate driven by accrued interest.
- Utilization-based kinked interest-rate curve compounded through a cumulative
  borrow-rate index; per-obligation scaled debt.
- Oracle-priced obligation health with loan-to-value and liquidation-threshold
  limits, and close-factor-capped liquidation with a seize bonus.
- Switchboard-On-Demand-shaped price feed with a `set_price` test writer.
- Rust + LiteSVM integration tests covering supply/redeem, borrow/repay,
  withdraw, interest accrual, liquidation, the share-inflation guard, and
  rounding/stale-input edge cases.
- Lending markets are isolation boundaries: every obligation handler rejects
  reserves from another market (`MarketMismatch`).
- Price feed PDAs are seeded by their authority, so no signer can write or
  pre-claim a feed another authority's reserves trust.
- Liquidation reads the close factor from the repay reserve, the bonus from the
  collateral reserve, and rejects repayments whose seizure would exceed the
  posted collateral (`LiquidationTooLarge`).
- Withdraw health checks round the removed borrow power up, so independent
  rounding can never let a withdraw pass that an exact recompute would reject.
- Reserve factor: the protocol keeps `reserve_factor_bps` of accrued interest as
  fees the market owner withdraws with `collect_protocol_fees`; the fees are
  carved out of `total_liquidity` so they never inflate the supplier exchange rate.
- LendingMarket is seeded by a `market_id` index (`["lending_market", market_id]`),
  not by any individual; one owner can run several independent markets, and admin
  handlers authorize via `has_one = owner`.
- Price feeds are seeded `["price_feed", market, mint]` (scoped to a market, not
  to an individual); only the market owner may write one (`has_one = owner`).
