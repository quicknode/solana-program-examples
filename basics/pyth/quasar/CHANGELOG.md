# Changelog

## [2026-07-22]

### Changed

- Migrated to Quasar 0.1.0 (`0.1.0-release` branch, rev `be60fca`): Quasar.toml
  rewritten to the 0.1.0 schema, `idl-build` feature added, and tests rewritten
  from the direct QuasarSVM harness to `quasar-test` (`#[quasar_test]` fixtures,
  the generated `ReadPriceInstruction` builder, `Outcome` assertions). The
  previously floating `quasar-lang`/`quasar-svm` git dependencies are now
  pinned (`quasar-svm` is gone entirely). The hand-built mock Pyth
  `PriceUpdateV2` oracle account (owner + raw byte layout) is preserved via
  `test.set_account(Account::new(...))`.

## 2026-07-07

Added this changelog. Changes prior to this date were tracked in git history only.
