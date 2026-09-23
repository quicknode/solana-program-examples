# Changelog

## [2026-09-23]

### Changed

- Ported the Anchor v2 copy's draft state and betting close time. Events start
  as `Draft` and move to `Open` through a new admin handler, `open_betting`,
  which needs at least two outcomes (`NotEnoughOutcomes`). `add_outcome` works
  only on a draft (`EventNotDraft`, replacing `BettingAlreadyStarted`), and
  `place_bet` on a draft fails with `EventNotOpen`, so the outcome list is fixed
  before any bet can land.
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
