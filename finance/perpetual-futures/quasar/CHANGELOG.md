# Changelog

## Unreleased, 2026-10-05

Every fee rounds up. `basis_points_of` in `instructions/shared.rs` rounds its
result up to the next base unit, so the open, close and liquidation fees and
the maintenance requirement a position is liquidated at each round in the
pool's favour: a fee is never a minor unit short, and a position is never a
minor unit too healthy to liquidate. The insurance fund's cut of a fee the
pool already holds is split by the new `basis_points_of_rounded_down`, and the
program takes the remainder, so the two still add up to the whole fee. Tested
by `fees_and_maintenance_requirement_round_up`, which opens, closes and
liquidates a position one base unit over $5,000 and checks the 5,000,001-unit
fees, the 2,500,000 / 2,500,001 insurance and program split, the liquidation
at an equity of exactly 250,000,001 and the 50,000,001 liquidation fee, and by
`basis_points_of_rounds_up_and_the_insurance_split_rounds_down` at the
boundaries.

Every refusal test asserts its error code:
`open_rejects_price_from_before_a_restart` asserts `PRICE_PREDATES_RESTART`
(18) and `wide_oracle_confidence_is_rejected` `ORACLE_CONFIDENCE_TOO_WIDE`
(16). `initialize_pool_rejects_close_fee_at_or_above_maintenance_margin` runs
a close fee of 600 and of 500 basis points against a 500 maintenance margin,
both refused with `INVALID_PARAMETER`, and 499, accepted.

## 2026-10-04

Check which program owns the price feed. `initialize_pool` records the feed
account's owning program on the new `Pool.price_feed_program`, read from the
account's owner at that moment, beside the feed address and scale it already
pins. `read_feed_price` takes that program and refuses a feed account owned by
any other with the new `PRICE_FEED_NOT_FROM_ORACLE` (23), before it decodes a
byte, so no handler accepts any account laid out like a feed as a price.
Tested by `open_rejects_price_feed_from_another_program`, which rewrites the
feed as a byte-identical copy owned by an unrelated program and then restores
the owner; `initialize_pool_creates_pool_vault_and_lp_mint` checks the
recorded program.

`funding_follows_seconds_not_slots` asserts the exact funding paid, `size *
rate * seconds / FUNDING_PRECISION`, rather than that some was paid, and the
new `first_deposit_below_minimum_fails` asserts `DEPOSIT_TOO_SMALL` by code one
base unit under the withheld minimum and a single share minted one over it.

## 2026-10-01

Replace the leverage cap with an initial margin. `initialize_pool`'s
`max_leverage` argument and `Pool::max_leverage` are now `initial_margin_bps`,
the net collateral a position must post to open, in basis points of its size
(1,000 is 10x). `initialize_pool` requires `maintenance_margin_bps <
initial_margin_bps <= 10_000`, refusing an initial margin at or below the
maintenance margin with the new `INITIAL_MARGIN_NOT_ABOVE_MAINTENANCE` (19) and
one above 10,000 with `INVALID_PARAMETER`; `MAX_LEVERAGE_CEILING` is removed.
`open_position` checks `net_collateral * 10_000 >= size * initial_margin_bps`
and fails with `INITIAL_MARGIN_NOT_MET`, which takes `LEVERAGE_TOO_HIGH`'s code
(2). Its separate check that a new position starts above the maintenance margin
is removed, because the initial margin implies it; `POSITION_NOT_HEALTHY`
remains for `close_position`. An open fee larger than the posted collateral now
fails with `INSUFFICIENT_COLLATERAL` (17), as in the Anchor version, rather than
`INSUFFICIENT_LIQUIDITY`.

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
max_price_deviation_bps` with the new `PRICE_OUTSIDE_BAND` (21), checked against
the stored average before anything is folded in. `liquidate_position` folds and
records without the check. The new permissionless `update_price_average`
handler (discriminator 7) folds and records too, also without the check, so
keepers calling it repeatedly as time passes can walk the average to a genuine
move. `max_price_deviation_bps` is a
new `initialize_pool` argument, which must be above zero and below 10,000, or
the handler fails with the new `INVALID_PRICE_DEVIATION` (20). `shared.rs` has
`refresh_price_and_funding_within_band` for the four band-checked handlers
beside `refresh_price_and_funding` for the other two.

Tested by `open_rejects_position_below_initial_margin` (formerly
`open_rejects_excess_leverage`, now checking both sides of the boundary),
`initialize_pool_records_the_margins_band_and_average`,
`initialize_pool_rejects_initial_margin_at_or_below_maintenance`,
`initialize_pool_rejects_price_deviation_outside_range`,
`open_rejected_when_oracle_jumps_outside_band`,
`close_rejected_when_oracle_jumps_outside_band`,
`liquidity_changes_rejected_when_oracle_jumps_outside_band`,
`liquidation_runs_outside_band`, `price_average_catches_up_after_genuine_move`,
`single_update_moves_average_by_elapsed_fraction` and
`one_manipulated_read_after_idle_does_not_move_average`. The default test pool
uses a 1,000 basis point initial margin and a 2,000 basis point band;
`profit_runs_uncapped_when_backed` triples the price, far outside the
band, so it now calls `update_price_average` to record the new price, lets a
full window pass, and calls it again before closing.

Replace reserved liquidity with the haircut risk model from
[Percolator](https://github.com/aeyakovenko/percolator): trader collateral is
senior, and trader profit is junior, paid only as far as the pool can back it.
`Pool::reserved_liquidity` is removed, and with it `open_position`'s
`reserved + size <= liquidity` check, which failed with
`INSUFFICIENT_LIQUIDITY`, and `close_position`'s cap on profit at the
position's size. A position opens whatever the pool's liquidity, and profit has
no cap. `close_position` computes the haircut ratio `h = min(1, (liquidity +
insurance_fund) / max(0, traders' aggregate unrealized profit, closing
position's profit))` from the per-side accumulators, before the closing
position leaves them, and pays a winning position `profit * h /
HAIRCUT_PRECISION`, rounded down, with the new constant at 10^9, so every
winner closing at the same moment is paid the same fraction; a loss settles in
full. A winner who closes while open losers still offset them is paid at most
the pool's backing rather than refused, and every other winner's fraction is
unchanged. The profit is paid from `liquidity` first and from the insurance
fund for the rest; `POOL_INSOLVENT` remains as a defensive check. `remove_liquidity` caps a withdrawal at `liquidity` rather
than `liquidity - reserved_liquidity`, still failing with
`INSUFFICIENT_LIQUIDITY`. `shared.rs` has the new `haircut_ratio` and
`apply_haircut`.

Add an insurance fund. `Pool::insurance_fund` is new, and so is the
`initialize_pool` argument `insurance_fee_bps`, which must be below 10,000 or
the handler fails with `INVALID_PARAMETER`. That fraction of every open and
close fee goes to the fund, rounded down, and the rest to `program_fees`,
through the new `split_fee` and `credit_fee` in `shared.rs`.
`liquidate_position` takes a position's deficit, its loss beyond its
collateral, from the fund first and credits what the fund pays to `liquidity`;
the providers bear the rest. The liquidation fee is still paid only out of the
position's remaining equity: the part the equity cannot cover is forgiven, as
in Percolator, and neither the insurance fund nor `liquidity` pays it. The
vault holds `liquidity + total_collateral + program_fees + insurance_fund`,
plus any tokens sent to it directly.

Add a profit warm-up. The `initialize_pool` argument `profit_warmup_slots`,
after `insurance_fee_bps`, and `Position::entry_slot`, which `open_position`
sets to the current slot, are new. `close_position` refuses to pay a profit
before slot `entry_slot + profit_warmup_slots` with the new
`PROFIT_NOT_MATURED` (22). A losing position closes at any time, and
liquidation is not delayed.

Tested by `open_allowed_without_full_backing`,
`profit_runs_uncapped_when_backed`, `haircut_scales_profit_when_pool_stressed`,
`insurance_pays_profit_beyond_liquidity`,
`winner_offset_by_open_loser_is_paid_not_refused`,
`remove_liquidity_capped_at_liquidity`, `profit_blocked_before_maturation`,
`profit_realized_after_maturation`, `loss_not_gated_by_maturation`,
`insurance_fund_funded_by_fees`, `insurance_absorbs_bankruptcy_deficit`,
`liquidation_of_bankrupt_position_charges_insurance_before_liquidity` and
`initialize_pool_rejects_insurance_fee_at_or_above_full_fee`. They replace
`open_rejects_when_pool_cannot_back_it`,
`profit_is_capped_at_the_reserved_notional` and
`remove_liquidity_is_blocked_by_reserved_notional`. The default test pool pays
half of each fee into the insurance fund and has a 10-slot warm-up, so the
tests that close at a profit first let the warm-up pass, and
`collect_fees_sweeps_the_open_fee_to_the_admin` sweeps the program's half.

## 2026-09-30

Remove `set_funding_rate` (discriminator 7). The pool's authority could change
the funding rate at any time, with no upper bound. The lighter side of open
interest is paid funding out of `liquidity`, so the authority could hold a small
position on that side from any wallet, raise the rate, and close it to take the
liquidity providers' deposits. The rate is now fixed by `initialize_pool`, which
refuses a rate above `MAX_FUNDING_RATE_PER_SECOND` (277, just under 0.1% of a
position's size per hour) with `INVALID_PARAMETER`.

Tested by `initialize_pool_rejects_funding_rate_above_the_maximum` and
`operator_on_the_lighter_side_earns_only_the_fixed_rate`.
`set_funding_rate_settles_at_the_old_rate_first` and
`only_the_authority_can_set_the_funding_rate` are removed with the handler.

## 2026-09-23

Documentation only: a production feed is now described as a Pyth
`PriceUpdateV2` account, since the oracle network the test feed was modeled on
has shut down.

## 2026-09-22

Accrue funding by the wall clock instead of by slots. The rate was quoted per
slot, so what a position cost per hour moved with the cluster's slot time, and
the reduction to 200 ms slots doubled it. `Pool::funding_rate_per_slot` is now
`funding_rate_per_second`, and `last_funding_slot` is now
`last_funding_timestamp`, the Clock's `unix_timestamp` at the last accrual; the
same rename applies to `initialize_pool`'s and `set_funding_rate`'s arguments. A
timestamp at or before the stored one accrues nothing. `advance_funding` is
replaced by `accrue_funding`, which updates the pool in place as the Anchor
version's does. Tested by `funding_follows_seconds_not_slots`, with
`set_funding_rate_settles_at_the_old_rate_first` and
`inflating_liquidity_through_own_trades_does_not_pay` now counting seconds.

`add_liquidity` and `remove_liquidity` now divide by the share supply plus
`MINIMUM_LIQUIDITY`, so the 1,000 shares withheld from the first deposit belong
to nobody and their slice of the pool stays locked. Before, both divided by the
bare supply, and a provider who was also the only trader could pay funding into
`liquidity` to inflate their single share and take part of the next deposit. A
pool whose providers have all left now prices the next deposit against the
locked slice. `inflating_liquidity_through_own_trades_does_not_pay` runs the
attack.

## 2026-09-10

Remove the separate dataless signing PDA (seeds `["authority", pool]`) that
owned the vault and the LP mint, its seeds struct, and the bump field on `Pool`
that recorded it. The pool account is already a PDA, so it is now the custody
vault's owner and the LP mint's authority itself, and signs vault transfers and
mint/burn CPIs with its own seeds, `["pool", collateral_mint, oracle_feed,
bump]`: the pattern the escrow example uses for its vault and the vault-strategy
example uses for its share mint. `initialize_pool`, `add_liquidity`,
`remove_liquidity`, `close_position`, `liquidate_position` and `collect_fees`
each take one account fewer. The admin signer stored on `Pool` as `authority`
(the pool operator) is unchanged.
`initialize_pool_creates_pool_vault_and_lp_mint` now also checks that the
vault's owner and the mint's authority are the pool.

## 2026-08-14

Add `set_funding_rate` (discriminator 7), so the pool operator can retune
`funding_rate_per_slot` after the pool is created. The rate is quoted per slot,
so what a position costs per hour depends on the cluster's slot time as well as
on the rate; Solana lowers the slot time over time, and a pool created before a
reduction charges the heavier side more per hour than it was set up to. The
handler advances the funding index at the old rate before storing the new one,
so slots already elapsed are charged at the rate that was in force for them.
Tested by `set_funding_rate_settles_at_the_old_rate_first` and
`only_the_authority_can_set_the_funding_rate`.

Also drop the "at 400ms" gloss from the price-staleness constant: the window is
counted in slots on purpose, and what it comes to in seconds follows the
cluster.

## 2026-08-04

Reject oracle prices from before a cluster restart: `read_oracle_price`
requires the feed's slot to be after the `LastRestartSlot` sysvar's slot
(`PRICE_PREDATES_RESTART`). quasar-lang has no LastRestartSlot sysvar, so
`src/last_restart.rs` declares the layout and reads it via
`sol_get_sysvar`. Also pinned `zeropod = "=0.3.3"` (zeropod 0.3.4 moved to
wincode 0.5 while quasar-lang's pinned rev stays on wincode 0.4, so a fresh
resolve failed every Pod* trait bound). Tested by
`open_rejects_price_from_before_a_restart`.

## [2026-07-22]

### Changed

- Migrated to Quasar 0.1.0 (`0.1.0-release` branch, rev `be60fca`): Quasar.toml
  rewritten to the 0.1.0 schema, `idl-build` feature and `lib` crate-type added,
  and tests rewritten from the direct QuasarSVM harness to `quasar-test`
  (`#[quasar_test]` fixtures, `crate::cpi` instruction builders, `Outcome`
  assertions; the hand-crafted oracle feed account is injected via
  `test.set_account`). The `quasar-svm` git dev-dependency is gone. Program-
  source fix for 0.1.0: `Seed` is no longer in the prelude, so the instruction
  files that build signer seeds now import it from `quasar_lang::cpi`.

## 2026-07-07

Added this changelog. Changes prior to this date were tracked in git history only.
