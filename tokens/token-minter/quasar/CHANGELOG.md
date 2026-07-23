# Changelog


## [2026-07-23]

### Changed

- Migrated to Quasar 0.1.0 (`0.1.0-release` branch, rev `be60fca`): Quasar.toml
  rewritten to the 0.1.0 schema, `idl-build` feature and `lib` crate-type added,
  and tests rewritten from the direct QuasarSVM harness to `quasar-test`
  (`#[quasar_test]` fixtures, `crate::cpi` instruction builders, `Outcome`
  assertions). The `quasar-svm` git dev-dependency is gone; compute-unit
  assertions were dropped pending recalibration under 0.1.0.
- `quasar-metadata` now resolves to the vendored copy at
  `tokens/quasar-metadata` (path dependency): upstream removed the crate
  before the 0.1.0 release with no replacement. See
  `tokens/quasar-metadata/README.md`.
- Tests still exercise `mint_token` only: the quasar-test harness ships no
  Metaplex Token Metadata program, so `create_token` (metadata CPI) remains
  untestable in-SVM, matching the previous suite.

## 2026-07-07

Added this changelog. Changes prior to this date were tracked in git history only.
