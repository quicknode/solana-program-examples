# Changelog

## [2026-07-22]

### Changed

- Migrated to Quasar 0.1.0 (`0.1.0-release` branch, rev `be60fca`): Quasar.toml
  rewritten to the 0.1.0 schema, `idl-build` feature and `lib` crate-type added,
  and tests rewritten from the QuasarSVM harness + generated-client path
  dev-dependency to `quasar-test` (`#[quasar_test]` fixtures, `crate::cpi`
  instruction builders, `Outcome` assertions, `test.warp_to_timestamp` for the
  deadline scenarios). The `quasar-svm` git dev-dependency and the
  `quasar-token-fundraiser-client` path dev-dependency are gone. Program-source
  fix for 0.1.0: `Seed` is no longer in the prelude, so
  `check_contributions.rs` and `refund.rs` now import it from
  `quasar_lang::cpi`.

## 2026-07-07

Added this changelog. Changes prior to this date were tracked in git history only.
