# Changelog

## [2026-07-22]

### Changed

- Migrated to Quasar 0.1.0 (`0.1.0-release` branch, rev `be60fca`): Quasar.toml
  rewritten to the 0.1.0 schema, `idl-build` feature added, and tests rewritten
  from the direct QuasarSVM harness to `quasar-test` (`#[quasar_test]`
  fixtures, `crate::cpi` instruction builders, `Outcome` assertions with
  token-balance and mint-supply checks). The `quasar-svm`,
  `spl-token-interface`, and `solana-program-pack` dev-dependencies are gone;
  compute-unit prints were dropped pending recalibration under 0.1.0.

## 2026-07-07

Added this changelog. Changes prior to this date were tracked in git history only.
