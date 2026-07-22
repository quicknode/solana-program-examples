# Changelog

## 2026-07-22

Renamed `Config.fee_bps` to `Config.default_fee_bps` (and the matching
`initialize_config` argument), mirroring the Anchor build. The config's value
is only a default copied into each new event's `fee_bps` at creation;
settlement charges the event's copy, so the old name overstated what the config
field did. Account layouts and instruction data encoding are unchanged. Also
fixed the README's port-notes bullet, which mentioned a `side` field this
program does not have (left over from the perpetual-futures port notes).

## 2026-07-07

Added this changelog. Changes prior to this date were tracked in git history only.
