# Changelog

## [Unreleased] - 2026-10-04

### Added

- Two admin handlers close what an event created, once it is `Settled` or
  `Cancelled` and every Bet account is closed. `close_outcome` closes one
  Outcome account and returns its rent to the admin. `close_event`, once every
  Outcome account is closed, pays whatever the vault still holds (the rounding
  dust a settlement's floored payouts leave; nothing after a cancellation) to
  the fee recipient's token account, closes the vault, and closes the Event
  account, returning both rents to the admin. Errors `EventNotFinished`,
  `BetsStillOpen` and `OutcomesStillOpen`, appended after `BettingStillOpen`.
  `close_outcome` takes discriminator 10 and `close_event` 11, leaving the
  existing ones in place.
- `Event.open_bets` counts the Bet accounts still open across every outcome:
  `place_bet` adds one when it creates a Bet account (a top-up reuses the
  account), and `claim_winnings`, `claim_refund` and `close_losing_bet` each
  subtract one. `Event.open_outcomes` counts the Outcome accounts still open:
  `add_outcome` adds one, `close_outcome` subtracts one. Both fields sit after
  `outcome_count`, so the Event account layout changes. `claim_winnings`,
  `claim_refund` and `close_losing_bet` now take the event as writable.
- Tests: `close_event_pays_dust_to_fee_recipient_and_returns_rent`,
  `close_event_refused_while_a_bet_is_open`,
  `close_event_refused_while_event_is_open` and
  `only_admin_can_close_outcomes_and_event`;
  `cancelled_event_refunds_the_exact_stake` refunds a second bettor and closes
  the cancelled event after its refunds.

### Changed

- `initialize_event_rejects_a_non_admin_signer` also sends `add_outcome` from
  a non-admin, and `full_lifecycle_settles_and_pays_the_winner` asserts
  `EventNotSettled` for a close before settlement, `BetWon` for closing the
  winning bet as a loser, and `NothingToClaim` for the loser's claim.

## [2026-10-03]

### Removed

- The per-wallet `User` index account (`seeds = [b"user", wallet]`), its
  `MAX_BETS_PER_USER` cap of 32 open positions, and the `TooManyBets` and
  `BetNotInUserIndex` errors. `place_bet`, `claim_winnings`, `claim_refund` and
  `close_losing_bet` no longer take a `user` account. A client lists a wallet's
  open bets with `getProgramAccounts` and a `memcmp` filter on `Bet.bettor` at
  offset 1, so a wallet can hold any number of open positions
  (`no_cap_on_open_bets_per_wallet`). Error codes after `ZeroAmount` move down by two.
- Outcome labels in the tests are invented film titles.

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

## [2026-09-08]

### Changed

- Renamed `Config.fee_bps` to `Config.default_fee_bps` (and the matching
  `initialize_config` argument), mirroring the Anchor builds. The config's
  value is only a default copied into each new event's `fee_bps` at creation;
  settlement charges the event's copy, so the old name overstated what the
  config field did. Account layouts and instruction data encoding are
  unchanged. Also fixed the README's port-notes bullet, which mentioned a
  `side` field this program does not have (left over from the
  perpetual-futures port notes).

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
