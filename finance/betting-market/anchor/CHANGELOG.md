# Changelog

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
