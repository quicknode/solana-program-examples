# Changelog

## [Unreleased] - 2026-10-05

### Changed

- `settle_event` rounds the fee up: `fee = ceil(losing_pool * fee_bps / 10_000)`,
  and `distributable_losing_pool` is the losing pool minus that fee, so the
  rounding on the fee goes the program's way as the rounding on each payout
  does. A losing pool that is an exact multiple of the fee rate pays the same
  fee as before; any other pays one minor unit more. The Kani harness
  `proof_settlement_fee_and_split` proves the fee is that ceiling.
- `test_double_claim_fails` asserts the error the second claim hits: Anchor's
  own account check, reported as the runtime's `UninitializedAccount`, since the first claim closed the Bet account.

### Added

- `test_only_admin_can_settle_or_cancel_event`: `settle_event` and
  `cancel_event` from a non-admin both fail with `Unauthorized`, the event stays
  `Open`, and the admin then settles it.

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
- `Event.open_bets` counts the Bet accounts still open across every outcome:
  `place_bet` adds one when it creates a Bet account (a top-up reuses the
  account), and `claim_winnings`, `claim_refund` and `close_losing_bet` each
  subtract one. `Event.open_outcomes` counts the Outcome accounts still open:
  `add_outcome` adds one, `close_outcome` subtracts one. Both fields sit after
  `outcome_count`, so the Event account layout changes. `claim_winnings`,
  `claim_refund` and `close_losing_bet` now take the event as writable.
- Tests: `test_close_event_pays_dust_to_fee_recipient_and_returns_rent`,
  `test_close_event_refused_while_a_bet_is_open`,
  `test_close_event_refused_while_event_is_open` and
  `test_only_admin_can_close_outcomes_and_event`; `test_cancel_and_refund`
  closes the cancelled event after its refunds.

### Changed

- `test_only_admin_can_initialize_event` asserts the `Unauthorized` code and
  also sends `add_outcome` and `open_betting` from a non-admin;
  `test_close_losing_bet_only_after_settle_and_only_for_losers` asserts
  `EventNotSettled` and `BetWon` by code, and `test_full_lifecycle` asserts
  `NothingToClaim` for the loser's claim.

## [2026-10-03]

### Removed

- The per-wallet `User` index account (`seeds = [b"user", wallet]`), its
  `MAX_BETS_PER_USER` cap of 32 open positions, and the `TooManyBets` and
  `BetNotInUserIndex` errors. `place_bet`, `claim_winnings`, `claim_refund` and
  `close_losing_bet` no longer take a `user` account. A client lists a wallet's
  open bets with `getProgramAccounts` and a `memcmp` filter on `Bet.bettor` at
  offset 8, so a wallet can hold any number of open positions
  (`test_no_cap_on_open_bets_per_wallet`). Error codes after `ZeroAmount` move down by two.
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

## 2026-09-08

Renamed `Config.fee_bps` to `Config.default_fee_bps` (and the matching
`initialize_config` argument). The config's value is only a default copied into
each new event's `fee_bps` at creation; settlement charges the event's copy, so
the old name overstated what the config field did. The event's `fee_bps` keeps
its name because it is the fee actually charged. Account layouts are unchanged;
only the IDL field/argument names differ.

## 2026-07-07

Added this changelog. Changes prior to this date were tracked in git history only.
