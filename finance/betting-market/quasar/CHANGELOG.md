# Changelog

## [2026-09-22]

### Changed

- Events start as `Draft` and move to `Open` through a new admin handler,
  `open_betting`, which needs at least two outcomes (`NotEnoughOutcomes`).
  `add_outcome` works only on a draft (`EventNotDraft`, replacing
  `BettingAlreadyStarted`), and `place_bet` on a draft fails with
  `EventNotOpen`, so the outcome list is fixed before any bet can land.
- `initialize_event` takes `betting_closes_at` (after `event_id`, before the
  description), which must be in the future (`CloseTimeInPast`). `place_bet`
  requires `now < betting_closes_at` (`BettingClosed`) and `settle_event`
  requires `now >= betting_closes_at` (`BettingStillOpen`).
- `cancel_event` accepts a draft as well as an open event.
- `EventStatus` gains `Draft = 0`, shifting `Open`, `Settled`, and `Cancelled`
  to 1, 2, and 3 so they keep matching the Anchor build's borsh encoding.
  `open_betting` takes discriminator 9, leaving the existing ones in place.

## [2026-07-22]

### Changed

- Migrated to Quasar 0.1.0 (`0.1.0-release` branch, rev `be60fca`): Quasar.toml
  rewritten to the 0.1.0 schema, `idl-build` feature and `lib` crate-type added,
  and tests rewritten from the direct QuasarSVM harness to `quasar-test`
  (`#[quasar_test]` fixtures, `crate::cpi` instruction builders, `Outcome`
  assertions). The `quasar-svm` git dev-dependency is gone; compute-unit
  assertions were dropped pending recalibration under 0.1.0. Program-source
  fixes for 0.1.0: `Seed` is now imported from `quasar_lang::cpi` in
  `instructions/shared.rs`, and the self-referential `Bet` PDA constraint
  (`Bet::seeds(&bet.outcome, ...)`) became
  `Bet::find_address(bet.outcome, *bettor.address(), &crate::ID)` — the same
  canonical-PDA check, expressed in a form 0.1.0's client codegen supports.

## 2026-07-07

Added this changelog. Changes prior to this date were tracked in git history only.
