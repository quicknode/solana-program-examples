# Changelog

## 2026-07-22

Renamed `Config.fee_bps` to `Config.default_fee_bps` (and the matching
`initialize_config` argument). The config's value is only a default copied into
each new event's `fee_bps` at creation; settlement charges the event's copy, so
the old name overstated what the config field did. The event's `fee_bps` keeps
its name because it is the fee actually charged. Account layouts are unchanged;
only the IDL field/argument names differ.

## 2026-07-07

Added this changelog. Changes prior to this date were tracked in git history only.
