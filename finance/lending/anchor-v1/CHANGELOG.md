# Changelog

## Unreleased (2026-10-07)

Round interest against the borrower. Every debt is `borrowed_principal`
times `borrow_accumulation_factor`, and the debt itself was already ceiled,
but the arithmetic that grows the factor floored at every step, each time in
the borrower's favor. Five divisions now round up with `mul_div_ceil`:
`Reserve::utilization_bps` (its only use is the borrow rate), both segments of
the kinked-curve interpolation in `current_borrow_rate_per_second`, the
conversion of that APR to a per-second rate, and the factor update in
`accrue_interest`. The rate still stays within `[min, max]` and utilization
within 10,000 bps. Suppliers are not overpaid by it: the reserve counts its
debt as `borrowed_principal` times the same factor, ceiled once, which is never
more than the sum of the borrowers' individually ceiled debts, so the pool's
assets never include interest no borrower owes; redemptions still floor and
the program fee still rounds up. On the book's walkthrough (750 borrowed from
a 2,000 USDC pool for five weeks) Bob's debt rises from 757.501028 to
757.508220 USDC. Tested by
`accumulation_factor_rounds_up_against_the_borrower`, which runs a second
accrual from a factor no longer at 1.0, checks that flooring would have given
a smaller utilization, APR, rate and factor, and asserts the program's factor
is the ceiled one. The test helper `factor_after` in `test_reserve.rs` now
rounds up too.

## Unreleased (2026-10-05)

Cap the borrow rate, ratchet the liquidation threshold and keep the bonus
payable. The market owner's `update_reserve_config` acts at once on a reserve
with open loans, and could raise the rate curve to any u16 (655% a year) or
lower `liquidation_threshold_bps` and make existing borrowers liquidatable on
the spot. `ReserveConfig::validate` now refuses any of `min_borrow_rate_bps`,
`optimal_borrow_rate_bps` or `max_borrow_rate_bps` above the new
`BORROW_RATE_CEILING_BPS` (30,000 bps, 300% a year) with the new
`BorrowRateAboveCeiling` error, at `initialize_reserve` and on every update.
`update_reserve_config` also refuses a config whose
`liquidation_threshold_bps` is below the reserve's current value with the new
`RiskLimitLowered` error; raising it is allowed. `loan_to_value_bps` is not
ratcheted, because it limits only new borrows, so the owner can still lower it
to stop new borrowing against an asset. Because the threshold can now only
rise, `ReserveConfig::validate` also refuses a config where
`liquidation_threshold_bps * (10_000 + liquidation_bonus_bps)` exceeds
`10_000 * 10_000` with the new `LiquidationBonusUnpayable` error, so a
liquidation at the threshold can always pay its bonus out of the collateral.
Tested by `rejects_borrow_rate_above_ceiling_at_initialize`,
`rejects_borrow_rate_above_ceiling_on_update` (each rate field alone above the
ceiling), `accepts_borrow_rate_at_ceiling`,
`rejects_lowering_liquidation_threshold`, `accepts_lowering_loan_to_value`,
`accepts_raising_loan_to_value_and_liquidation_threshold`,
`accepts_curve_change_with_risk_limits_unchanged`,
`rejects_unpayable_liquidation_bonus_at_initialize`,
`rejects_unpayable_liquidation_bonus_on_update` and
`accepts_liquidation_bonus_at_the_bound`; `accepts_valid_config_update` now
raises the loan-to-value instead of lowering it. The tests gain a
`try_add_reserve_to` helper that returns the `initialize_reserve` result.

Close each collateral vault when its last share leaves. A withdrawal or a
liquidation that empties a reserve's deposit entry now also closes that
reserve's per-obligation share vault, whose rent the obligation's owner paid
in `deposit_obligation_collateral` (`init_if_needed` recreates it on a later
deposit). The vault's whole balance moves out first, to the owner on a
withdrawal and to the liquidator on a liquidation, so share tokens donated
straight to the vault cannot keep it open or make the withdrawal fail. A
withdrawal closes the vault to the owner, whose `owner` account is now
writable. A liquidation closes it into the obligation account, and
`close_obligation` returns that rent to the owner with the obligation's own;
`liquidate_obligation`'s accounts are unchanged. Tested by
`full_withdraw_closes_the_vault_and_returns_its_rent`,
`partial_withdraw_keeps_the_vault_open`,
`redeposit_after_full_withdraw_recreates_the_vault`,
`donated_shares_cannot_keep_the_vault_open`,
`seizing_all_collateral_closes_the_vault_into_the_obligation`,
`seizing_all_collateral_sweeps_donated_shares_to_the_liquidator`,
`partial_liquidation_keeps_the_vault_open`,
`close_obligation_after_full_liquidation_returns_both_rents_to_the_owner`,
`close_obligation_refused_while_debt_remains_after_full_liquidation`,
`redeposit_after_full_liquidation_recreates_the_vault`,
`owner_can_liquidate_their_own_obligation` and
`owner_can_liquidate_their_own_obligation_to_empty`;
`debt_free_withdraw_needs_no_price_and_no_refresh` now asserts the vault is
gone.

A debt-free borrower can always withdraw. `withdraw_obligation_collateral`
read the price feed and required a refreshed obligation on every call, and
`refresh_obligation` reads prices too, so a borrower with no borrows could not
take their collateral out while the feed was stale or silent. Now the handler
skips the refresh requirement, the price read and the health check when
`borrows` is empty, which is exactly when the debt is zero (a borrow entry is
removed when its last unit is repaid); the obligation is still marked stale so
its cached values are recomputed before the next health-dependent action.
Every check stays for an obligation with debt. Tested by
`debt_free_withdraw_needs_no_price_and_no_refresh`, which lets the price go
stale and withdraws the whole deposit with no refresh in the transaction,
`withdraw_after_full_repay_needs_no_price`, and
`withdraw_with_debt_is_refused_while_the_price_is_stale`, which asserts the
existing `StalePriceFeed` and `ObligationStale` refusals. The test harness
gains `try_withdraw_collateral_without_refresh`.

New `close_obligation` handler: closes an obligation with no deposits and no
borrows, returning its rent to the owner through `close = owner`; `has_one =
owner` refuses anyone else, and the new `ObligationNotEmpty` error refuses one
that still holds collateral or debt. Tested by
`close_obligation_returns_rent_to_owner` (rent back to the minor unit, account
gone), `close_obligation_with_collateral_is_refused`,
`close_obligation_with_debt_is_refused` and `non_owner_cannot_close_obligation`.

The program fee on accrued interest rounds up. `accrue_interest` computes the
reserve factor's cut of each accrual with `mul_div_ceil`, so when the cut is
not whole the extra unit goes to the market owner, and the suppliers take the
remainder; fee and remainder sum to the interest and never exceed it. A fee is
the program's cut and rounds against the user, as every fee in these programs
does. `program_fees_accrue_and_owner_can_collect` asserts the fee equals the
interest times the reserve factor, rounded up, and the new
`program_fee_rounds_up_and_suppliers_take_the_remainder` accrues one second of
interest on a 500-unit borrow (3 units) and checks the fee is 1 and the
suppliers' pool grows by 2. The Kani crate gains
`proof_program_fee_rounds_up_within_interest`, which checks the fee never
exceeds the interest for any reserve factor up to 100%.

`non_owner_cannot_write_market_price_feed` asserts the constraint error the
refusal raises (`ConstraintHasOne`), so every refusal test in the suite names
its error code.

## Unreleased (2026-10-04)

Refuse a price the oracle itself is unsure of. `PriceFeed` gains a
`confidence` field, the publisher's confidence band in the mantissa's units
(Pyth's `price_message.conf`), which `set_price` now takes as a third
argument. `ReserveConfig` gains `max_confidence_bps`, the widest band, as a
fraction of the price, the reserve will value against; `validate()` rejects a
limit above 10,000 or of zero, since a zero limit would refuse every live
price and freeze every obligation holding the asset. `price_scaled` now takes
the reserve's limit and fails with the new `OracleConfidenceTooWide` when
`confidence × 10,000 > price × max_confidence_bps`, computed in `u128` with
checked arithmetic, so `refresh_obligation`, `borrow_obligation_liquidity`,
`withdraw_obligation_collateral` and `liquidate_obligation` all refuse the
price. Tested by
`borrow_against_collateral_priced_with_a_wide_band_is_rejected`,
`borrow_of_a_token_priced_with_a_wide_band_is_rejected`,
`confidence_band_at_the_limit_passes_and_one_unit_over_fails`,
`rejects_confidence_limit_wider_than_the_price` and
`rejects_zero_confidence_limit`. The test harness's `set_price` publishes a
0.1% band; `set_price_with_confidence` takes one.

`deposit_redeem_round_trip_creates_no_value` now runs fifty deposit-and-redeem
round trips of 777,777,777 units against a reserve whose exchange rate interest
has moved off one-to-one, and asserts after each that the supplier holds no
more than they started with. It previously made one trip at a one-to-one rate,
where no rounding occurs.

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

The price feed's production path now points at a Pyth `PriceUpdateV2` account
(`price_mantissa = price_message.price`, `exponent = price_message.exponent`,
`last_updated_slot = posted_slot`), since the oracle network the feed was
modeled on has shut down. Documentation only: the account layout and handlers
are unchanged.

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
- Mantissa-and-exponent price feed with a `set_price` test writer.
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
