# Changelog

## [Unreleased] (2026-10-05)

### Changed

- Close the collateral vault when its last share leaves. A withdrawal or a
  liquidation that takes the last deposited share now also closes the
  obligation's share vault, rent to the obligation's owner, who paid it in
  `deposit_obligation_collateral` (`init(idempotent)` recreates it on a later
  deposit). The vault's whole balance moves out first, to the owner on a
  withdrawal and to the liquidator on a liquidation, so share tokens donated
  straight to the vault cannot keep it open or make the withdrawal fail.
  `liquidate_obligation` takes a new `obligation_owner` account
  (`address = obligation.owner`) to receive the rent. Tested by
  `full_withdraw_closes_the_vault_and_returns_its_rent`,
  `partial_withdraw_keeps_the_vault_open`,
  `redeposit_after_full_withdraw_recreates_the_vault`,
  `donated_shares_cannot_keep_the_vault_open`,
  `seizing_all_collateral_closes_the_vault_and_returns_its_rent_to_the_owner`
  and `liquidator_cannot_redirect_the_vault_rent` (`AddressMismatch`);
  `debt_free_withdraw_needs_no_price` now asserts the vault is gone.
- A debt-free borrower can always withdraw. `withdraw_obligation_collateral`
  valued the remaining collateral at the feed's price on every call, so a
  borrower with no debt could not take their collateral out while the feed
  was stale or silent. Now the handler reads no price and runs no health
  check when `borrowed_principal` is zero (repaying the last unit zeroes it);
  the price accounts are still passed and checked to be the reserves' own,
  but their values are not read. Every check stays for an obligation with
  debt. Tested by `debt_free_withdraw_needs_no_price`, which lets the price
  go stale and withdraws the whole deposit,
  `withdraw_after_full_repay_needs_no_price`, and
  `withdraw_with_debt_is_refused_while_the_price_is_stale`, which asserts the
  existing `StalePrice` refusal.
- The program fee on accrued interest rounds up. `accrue` computes the reserve
  factor's cut of each accrual with `mul_div_ceil`, so when the cut is not
  whole the extra unit goes to the market owner, and the suppliers take the
  remainder; fee and remainder sum to the interest and never exceed it. A fee
  is the program's cut and rounds against the user, as every fee in these
  programs does. The Kani crate gains
  `proof_program_fee_rounds_up_within_interest`, which checks the fee never
  exceeds the interest for any reserve factor up to 100%.
- `first_deposit_must_exceed_the_minimum`, `borrow_up_to_ltv_succeeds_and_beyond_fails`
  and `unhealthy_position_is_liquidated_and_healthy_is_rejected` assert the
  error each refusal raises (`DepositTooSmall`, `BorrowTooLarge`,
  `ObligationHealthy`), so every refusal test in the suite names its error code.

### Added

- `close_obligation` (discriminator 12): closes an obligation with no deposited
  shares and no borrowed principal, returning its rent to the owner through
  `close(dest = owner)`; `has_one(owner)` and the owner-seeded address refuse
  anyone else, and the new `ObligationNotEmpty` error refuses one that still
  holds collateral or debt. Tested by `close_obligation_returns_rent_to_owner`
  (rent back to the lamport, account gone),
  `close_obligation_with_collateral_is_refused`,
  `close_obligation_with_debt_is_refused` and `non_owner_cannot_close_obligation`
  (which asserts `InvalidPda`, the framework's refusal of an obligation whose
  address was not derived from the signer).
- `program_fee_rounds_up_and_suppliers_take_the_remainder` accrues one second
  of interest on a 500-unit borrow (3 units) through `collect_program_fees`
  and checks the owner receives 1 and the suppliers' pool grows by 2.

## [Unreleased] (2026-10-04)

### Changed

- Refuse a price the oracle itself is unsure of. `PriceFeed` gains a
  `confidence` field, the publisher's confidence band in the mantissa's units
  (Pyth's `price_message.conf`), which `set_price` now takes as a third
  argument. `Reserve` gains `max_confidence_bps`, the widest band, as a
  fraction of the price, the reserve will value against, which
  `initialize_reserve` takes as its last config argument; `validate_config`
  rejects a limit above 10,000 or of zero, since a zero limit would refuse
  every live price and freeze every obligation holding the asset.
  `price_scaled` now takes the reserve's limit and fails with the new
  `OracleConfidenceTooWide` when
  `confidence × 10,000 > price × max_confidence_bps`, computed in `u128` with
  checked arithmetic, so `borrow_obligation_liquidity`,
  `withdraw_obligation_collateral` and `liquidate_obligation` all refuse the
  price. Tested by
  `borrow_against_collateral_priced_with_a_wide_band_is_rejected`,
  `borrow_of_a_token_priced_with_a_wide_band_is_rejected`,
  `confidence_band_at_the_limit_passes_and_one_unit_over_fails`,
  `rejects_confidence_limit_wider_than_the_price` and
  `rejects_zero_confidence_limit`. The test harness's `set_price` publishes a
  0.1% band; `set_price_with_confidence` takes one.

### Added

- `deposit_redeem_round_trip_creates_no_value` runs fifty deposit-and-redeem
  round trips of 777,777,777 units against a reserve whose exchange rate
  interest has moved off one-to-one, and asserts after each that the supplier
  holds no more than they started with.

## [2026-09-23]

### Changed

- Documentation only: the price feed's production path now points at a Pyth
  price feed, since the oracle network it was modeled on has shut down.

### Fixed

- Lock a minimum number of reserve shares. The first deposit now mints
  `deposit - MINIMUM_SHARES` (1,000) shares, and deposit, redeem, collateral
  valuation, collateral withdrawal and liquidation all divide by the share
  supply plus that minimum (`math::total_shares`). The withheld shares belong
  to nobody, so their slice of the pool stays locked. A first deposit of
  `MINIMUM_SHARES` or less fails with `DepositTooSmall`, and a reserve whose
  suppliers have all left prices the next deposit against the locked slice.
  Recording `available_liquidity` stopped a vault donation, but total liquidity
  also counts interest owed on borrows, and a lone supplier who borrowed from
  their own reserve could raise their single share's value with that interest
  and then with rounding, until a later deposit rounded down in their favor.
  `inflating_shares_through_own_borrow_does_not_pay` runs that attack. Also
  tested by `first_deposit_withholds_the_minimum` and
  `first_deposit_must_exceed_the_minimum`; the test harnesses now open each
  reserve with a deposit from the market owner.

## [2026-09-22]

### Changed

- Accrue interest by the wall clock instead of by slots. The reserve's
  `slots_per_year` field was a guess at the cluster's slot length, and the
  test default of 78,840,000 (a 400 ms slot) charged twice the advertised APR
  once the network moved to 200 ms slots. Interest now accrues for the seconds
  between the Clock's `unix_timestamp` and the new
  `Reserve::last_accrual_timestamp`, at the APR divided by `SECONDS_PER_YEAR`;
  `slots_per_year` is gone from `Reserve` and from `initialize_reserve`'s
  arguments, and `borrow_rate_per_slot` is now `borrow_rate_per_second`. A
  timestamp at or before the stored one accrues nothing. `last_update_slot`
  stays, as the record of the last accrual's slot. Tested by
  `interest_accrues_by_seconds_not_slots` and
  `a_timestamp_behind_the_last_accrual_charges_nothing`, which replace
  `retuning_slots_per_year_rescales_accrual`. The low-level test module that
  moves the slot and the timestamp independently is now `clock_warp` (was
  `slot_warp`).

### Removed

- `update_slots_per_year` (discriminator 12). The field it retuned is gone.
  Discriminators are declared explicitly, so every other instruction keeps its
  number.

## [2026-08-14]

### Added

- `update_slots_per_year` (discriminator 12, owner-only): retunes a reserve to
  the cluster's current slot time. It accrues at the old figure before storing
  the new one, so slots already elapsed are charged at the rate that was in
  force for them. Every other config value is a policy choice the owner makes;
  this one tracks a protocol parameter that changes without asking, which is why
  it gets its own handler.

### Changed

- `Reserve` carries `slots_per_year`, and `initialize_reserve` takes it as a
  parameter. Converting an APR into a per-slot rate needs a slots-per-year
  divisor, and that divisor is the cluster's slot time in disguise; it was a
  `SLOTS_PER_YEAR` constant fixed at a 400ms slot, so a protocol change to the
  slot time would have raised the wall-clock rate every borrower pays with no
  code change. `validate_config` rejects zero. Tested by
  `retuning_slots_per_year_rescales_accrual`.

## [2026-08-04]

### Changed

- Reject oracle prices from before a cluster restart: `price_scaled` requires
  the feed's slot to be after the `LastRestartSlot` sysvar's slot
  (`PricePredatesRestart`). quasar-lang has no LastRestartSlot sysvar, so
  `src/last_restart.rs` declares the layout and reads it via
  `sol_get_sysvar`. Tested by
  `borrow_with_price_from_before_a_restart_is_rejected`.
- Pinned `zeropod = "=0.3.3"`: zeropod 0.3.4 moved to wincode 0.5 while
  quasar-lang's pinned rev stays on wincode 0.4, so a fresh resolve failed
  every Pod* trait bound.

## [2026-07-22]

### Changed

- Migrated to Quasar 0.1.0 (`0.1.0-release` branch, rev `be60fca`): Quasar.toml
  rewritten to the 0.1.0 schema, `idl-build` feature and `lib` crate-type added,
  and most tests rewritten from the direct QuasarSVM harness to `quasar-test`
  (`#[quasar_test]` fixtures, `crate::cpi` instruction builders, `Outcome`
  assertions). Compute-unit assertions were dropped pending recalibration
  under 0.1.0. Program-source fixes for 0.1.0: `Seed` is now imported from
  `quasar_lang::cpi`, and the removed `quasar_spl::initialize_account3` /
  `initialize_mint2` free functions became `TokenCpi` trait method calls on
  `token_program`. The two slot-warp scenarios
  (`interest_accrues_and_lifts_share_value`,
  `protocol_fees_accrue_and_owner_can_collect`) keep a direct
  `quasar-svm = "=0.1.0"` (crates.io) dev-dependency: interest accrual is
  computed from `Clock::get()?.slot`, and quasar-test exposes no slot warp
  (`warp_to_timestamp` only sets `unix_timestamp`), so they drive
  `QuasarSvm` + `sysvars.warp_to_slot` directly, loading the compiled `.so`
  at runtime.

## 0.1.0

Initial Quasar port of the Kamino/Solend-style borrow/lend program.

- Lending market, per-asset reserves with a program-owned liquidity vault and a
  share-token mint, and isolated single-collateral / single-borrow obligations.
- Share-token deposit accounting with an exchange rate driven by accrued interest.
- Utilization-based kinked interest-rate curve compounded through a cumulative
  borrow-rate index, accrued inline per instruction.
- Oracle-priced health with loan-to-value and liquidation-threshold limits, and
  close-factor-capped liquidation with a seize bonus.
- Mantissa-and-exponent price feed with a `set_price` test writer.
- quasar-svm integration tests covering supply/redeem, borrow/repay, interest
  accrual, and liquidation (including the healthy-rejection path).
- Price feed PDAs are seeded by their authority, so no signer can write or
  pre-claim a feed another authority's reserves trust.
- Liquidation reads the close factor from the borrow reserve, and rejects
  repayments whose seizure would exceed posted collateral
  (`LiquidationTooLarge`).
- Reserve factor: the protocol keeps `reserve_factor_bps` of accrued interest
  as fees the market owner withdraws with `collect_protocol_fees`.
- LendingMarket is seeded by a `market_id` index (`["lending_market", market_id]`),
  not by any individual; one owner can run several independent markets.
- Price feeds are seeded `["price_feed", market, mint]` (scoped to a market, not
  to an individual); only the market owner may write one.
