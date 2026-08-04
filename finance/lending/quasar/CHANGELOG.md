# Changelog

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
- Switchboard-On-Demand-shaped price feed with a `set_price` test writer.
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
