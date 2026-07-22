# Changelog

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
