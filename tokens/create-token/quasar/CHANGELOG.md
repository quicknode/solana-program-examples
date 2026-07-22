# Changelog

## [2026-07-22]

### Changed

- Migrated to Quasar 0.1.0 (`0.1.0-release` branch, rev `be60fca`): Quasar.toml
  rewritten to the 0.1.0 schema, `idl-build` feature added, and tests rewritten
  from the direct QuasarSVM harness to `quasar-test` (`#[quasar_test]`
  fixtures, `crate::cpi` instruction builders, `Outcome` assertions). The
  `quasar-svm` git dev-dependency is gone; compute-unit prints were dropped
  pending recalibration under 0.1.0.
- `initialize_mint2` is now invoked through quasar-spl's `TokenCpi` trait
  (`token_program.initialize_mint2(...)`); 0.1.0 removed the free-function
  re-export.

## 2026-07-07

Added this changelog. Changes prior to this date were tracked in git history only.
