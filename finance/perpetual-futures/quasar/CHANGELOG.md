# Changelog

## 2026-08-14

Add `set_funding_rate` (discriminator 7), so the pool authority can retune
`funding_rate_per_slot` after the pool is created. The rate is quoted per slot,
so what a position costs per hour depends on the cluster's slot time as well as
on the rate; Solana lowers the slot time over time, and a pool created before a
reduction charges the heavier side more per hour than it was set up to. The
handler advances the funding index at the old rate before storing the new one,
so slots already elapsed are charged at the rate that was in force for them.
Tested by `set_funding_rate_settles_at_the_old_rate_first` and
`only_the_authority_can_set_the_funding_rate`.

Also drop the "at 400ms" gloss from the price-staleness constant: the window is
counted in slots on purpose, and what it comes to in seconds follows the
cluster.

## 2026-08-04

Reject oracle prices from before a cluster restart: `read_oracle_price`
requires the feed's slot to be after the `LastRestartSlot` sysvar's slot
(`PRICE_PREDATES_RESTART`). quasar-lang has no LastRestartSlot sysvar, so
`src/last_restart.rs` declares the layout and reads it via
`sol_get_sysvar`. Also pinned `zeropod = "=0.3.3"` (zeropod 0.3.4 moved to
wincode 0.5 while quasar-lang's pinned rev stays on wincode 0.4, so a fresh
resolve failed every Pod* trait bound). Tested by
`open_rejects_price_from_before_a_restart`.

## [2026-07-22]

### Changed

- Migrated to Quasar 0.1.0 (`0.1.0-release` branch, rev `be60fca`): Quasar.toml
  rewritten to the 0.1.0 schema, `idl-build` feature and `lib` crate-type added,
  and tests rewritten from the direct QuasarSVM harness to `quasar-test`
  (`#[quasar_test]` fixtures, `crate::cpi` instruction builders, `Outcome`
  assertions; the hand-crafted oracle feed account is injected via
  `test.set_account`). The `quasar-svm` git dev-dependency is gone. Program-
  source fix for 0.1.0: `Seed` is no longer in the prelude, so the instruction
  files that build signer seeds now import it from `quasar_lang::cpi`.

## 2026-07-07

Added this changelog. Changes prior to this date were tracked in git history only.
