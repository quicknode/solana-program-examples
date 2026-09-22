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

## 2026-07-07

Added this changelog. Changes prior to this date were tracked in git history only.
