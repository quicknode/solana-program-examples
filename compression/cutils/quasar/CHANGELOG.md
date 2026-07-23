# Changelog

## [2026-07-22]

### Changed

- Migrated to Quasar 0.1.0 (`0.1.0-release` branch, rev `be60fca`): Quasar.toml
  rewritten to the 0.1.0 schema and the `idl-build` feature added. The
  previously-floating `quasar-lang` git dependency is now pinned by rev. The
  `quasar-svm` git dev-dependency (and the unused `solana-address` dev-dep) are
  gone, replaced by `quasar-test`; the tests file remains a placeholder (the
  Bubblegum/Account-Compression CPI flows are still covered by the Anchor
  twin's LiteSVM suite) and now points at the `quasar-test` porting path.
- `bubblegum_types::get_asset_id` now calls
  `quasar_lang::pda::try_find_program_address`; 0.1.0 renamed the former
  `based_try_find_program_address`.

## 2026-07-07

Added this changelog. Changes prior to this date were tracked in git history only.
