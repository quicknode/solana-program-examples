# Changelog

## 2026-10-01

Replace the leverage cap with an initial margin. `max_leverage` on
`PoolParameters` and `Pool` is now `initial_margin_bps`, the net collateral a
position must post to open, in basis points of its size (1,000 is 10x).
`initialize_pool` requires `maintenance_margin_bps < initial_margin_bps <=
10_000`, refusing an initial margin at or below the maintenance margin with the
new `InitialMarginNotAboveMaintenance` and one above 10,000 with
`InvalidParameter`; `MAX_LEVERAGE_CEILING` is removed. `open_position` checks
`net_collateral * 10_000 >= size * initial_margin_bps` and fails with
`InitialMarginNotMet`, which takes `LeverageTooHigh`'s place and its error code
(6004). Its separate check that a new position starts above the maintenance
margin is removed, because the initial margin implies it; `PositionNotHealthy`
remains for `close_position`.

Add a price band around a program-maintained average price. A fresh, confident
oracle print could still be wrong, and every handler traded at it. The pool now
keeps `average_price`, a time-weighted moving average of the oracle price,
`last_oracle_price`, the price at the most recent oracle read, and
`average_price_timestamp`. `initialize_pool` seeds the average and
`last_oracle_price` from the oracle. Every handler that reads the oracle credits
the seconds since the previous read to the price that read saw,
`average += (last_oracle_price - average) * min(elapsed,
PRICE_AVERAGE_WINDOW_SECONDS) / PRICE_AVERAGE_WINDOW_SECONDS`, with the new
constant at 600 seconds, and then records the price it read as
`last_oracle_price`. The price read now only counts from now, so a pool left
idle for a window or more cannot have its average set by one read of a
manipulated price: that price moves the average only if the oracle still shows
it at a later read, weighted by the seconds between the two reads.
`open_position`, `close_position`, `add_liquidity` and `remove_liquidity` refuse
a price outside `|price - average_price| * 10_000 <= average_price *
max_price_deviation_bps` with the new `PriceOutsideBand`, checked against the
stored average before anything is folded in. `liquidate_position` folds and
records without the check. The new permissionless `update_price_average`
handler folds and records too, also without the check, so keepers calling it
repeatedly as time passes can walk the average to a genuine move. `max_price_deviation_bps` is a new
`PoolParameters` field, which `initialize_pool` requires to be above zero and
below 10,000 with the new `InvalidPriceDeviation`. `shared.rs` has
`refresh_price_and_funding_within_band` for the four band-checked handlers
beside `refresh_price_and_funding` for the other two. The `errors` module is
public so the tests can match `PerpError` codes.

Tested by `test_open_rejects_position_below_initial_margin` (formerly
`test_open_rejects_excess_leverage`, now checking both sides of the boundary),
`test_initialize_pool_rejects_initial_margin_at_or_below_maintenance`,
`test_initialize_pool_rejects_price_deviation_outside_range`,
`test_open_rejected_when_oracle_jumps_outside_band`,
`test_close_rejected_when_oracle_jumps_outside_band`,
`test_liquidity_changes_rejected_when_oracle_jumps_outside_band`,
`test_liquidation_runs_outside_band`,
`test_price_average_catches_up_after_genuine_move`,
`test_single_update_moves_average_by_elapsed_fraction` and
`test_one_manipulated_read_after_idle_does_not_move_average`. The default test market
uses a 1,000 basis point initial margin and a 2,000 basis point band;
`test_profit_capped_at_reserved_notional` triples the price, far outside the
band, so it now calls `update_price_average` to record the new price, lets a
full window pass, and calls it again before closing.

## 2026-09-30

Remove `set_funding_rate`. The pool's authority could change the funding rate at
any time, with no upper bound. The lighter side of open interest is paid funding
out of `liquidity`, so the authority could hold a small position on that side
from any wallet, raise the rate, and close it to take the liquidity providers'
deposits. The rate is now fixed by `initialize_pool`, which refuses a rate above
`MAX_FUNDING_RATE_PER_SECOND` (277, just under 0.1% of a position's size per
hour) with `InvalidParameter`.

Tested by `test_initialize_pool_rejects_funding_rate_above_the_maximum` and
`test_operator_on_the_lighter_side_earns_only_the_fixed_rate`.
`test_set_funding_rate_settles_at_the_old_rate_first` and
`test_only_authority_can_set_funding_rate` are removed with the handler.

## 2026-09-23

The mock oracle program is now `mock-price-feed` (library and program
`mock_price_feed`), with the same program ID, instructions and
account layout. The oracle network it was modeled on has shut down, so the
production path described in `state/oracle.rs` now reads a Pyth
`PriceUpdateV2` account, as `basics/pyth` does. No behavior changes.

## 2026-09-22

Accrue funding by the wall clock instead of by slots. The rate was quoted per
slot, so what a position cost per hour moved with the cluster's slot time, and
the reduction to 200 ms slots doubled it. `Pool::funding_rate_per_slot` is now
`funding_rate_per_second`, and `last_funding_slot` is now
`last_funding_timestamp`, the Clock's `unix_timestamp` at the last accrual; the
same rename applies to `PoolParameters` and `set_funding_rate`'s argument. A
timestamp at or before the stored one accrues nothing. Tested by
`test_funding_follows_seconds_not_slots`, with
`test_set_funding_rate_settles_at_the_old_rate_first`,
`test_funding_charged_to_long` and
`test_inflating_liquidity_through_own_trades_does_not_pay` now counting
seconds.

`add_liquidity` and `remove_liquidity` now divide by the share supply plus
`MINIMUM_LIQUIDITY`, so the 1,000 shares withheld from the first deposit
belong to nobody and their slice of the pool stays locked. Before, both divided
by the bare supply, the withheld value was shared among the holders, and a
provider who was also the only trader could pay funding into `liquidity` to
inflate their single share and take part of the next deposit. A pool whose
providers have all left now prices the next deposit against the locked slice
rather than bootstrapping it. A sole provider's round trip returns the deposit
less the 1,000 minimum, and a new test runs the funding-based inflation attack.

## 2026-09-10

Remove the separate dataless signing PDA (seeds `["authority", pool]`) that
owned the vault and the LP mint, and the bump field on `Pool` that recorded it.
The pool account is already a PDA, so it is now the custody vault's owner and
the LP mint's authority itself, and signs vault transfers and mint/burn CPIs
with its own seeds, `["pool", collateral_mint, oracle_feed, bump]`: the pattern
the escrow example uses for its vault and the vault-strategy example uses for
its share mint. `initialize_pool`, `add_liquidity`, `remove_liquidity`,
`close_position`, `liquidate_position` and `collect_fees` each take one account
fewer. The admin signer stored on `Pool` as `authority` (the pool operator) is
unchanged. `test_initialize_pool` now also checks that the vault's owner and the
mint's authority are the pool.

## 2026-09-08

Move to Anchor 1.2.0, LiteSVM 0.16.0 and solana-kite 0.5.0. No program source
changed. Two tests (`test_stale_price_rejected`,
`test_funding_charged_to_long`) used to warp to an absolute slot; LiteSVM now
starts its clock at a mainnet-like slot rather than zero, so those warps moved
time backwards and the price never went stale and no funding accrued. They now
warp relative to the current slot, as the other tests already did.

## 2026-08-14

Add `set_funding_rate`, so the pool operator can retune `funding_rate_per_slot`
after the pool is created. The rate is quoted per slot, so what a position costs
per hour depends on the cluster's slot time as well as on the rate; Solana lowers
the slot time over time, and a pool created before a reduction charges the
heavier side more per hour than it was set up to. The handler advances the
funding index at the old rate before storing the new one, so slots already
elapsed are charged at the rate that was in force for them. Tested by
`test_set_funding_rate_settles_at_the_old_rate_first` and
`test_only_authority_can_set_funding_rate`.

Also drop the "at 400ms/slot" gloss from the price-staleness constant: the
window is counted in slots on purpose, and what it comes to in seconds follows
the cluster.

## 2026-08-04

Reject oracle prices from before a cluster restart. A halt stops the slot
count but not the wall clock, so after a restart a feed can look fresh in
slots while its price is hours old; with leverage that error is amplified
market-wide. `read_oracle_price` now also requires the feed's slot to be
after the `LastRestartSlot` sysvar's slot (`PricePredatesRestart`). Tested
by `test_open_rejects_price_from_before_a_restart`.

## 2026-07-07

Added this changelog. Changes prior to this date were tracked in git history only.
