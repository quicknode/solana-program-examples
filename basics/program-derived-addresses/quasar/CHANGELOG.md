# Changelog

## [2026-07-22]

### Changed

- Migrated to Quasar 0.1.0 (`0.1.0-release` branch, rev `be60fca`): Quasar.toml
  rewritten to the 0.1.0 schema, `idl-build` feature and `lib` crate-type added,
  and tests rewritten from the direct QuasarSVM harness to `quasar-test`
  (`#[quasar_test]` fixtures, `crate::cpi` instruction builders, `Outcome`
  assertions). The `quasar-svm` git dev-dependency and the generated-client
  path dev-dependency are gone; state is now verified with typed
  `test.read::<PageVisits>` reads alongside the byte-layout checks.

## 2026-07-07

Added this changelog. Changes prior to this date were tracked in git history only.
