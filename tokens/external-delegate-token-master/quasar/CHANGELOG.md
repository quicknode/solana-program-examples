# Changelog

## [2026-07-22]

### Changed

- Migrated to Quasar 0.1.0 (`0.1.0-release` branch, rev `be60fca`): Quasar.toml
  rewritten to the 0.1.0 schema, `idl-build` feature added, and tests rewritten
  from the direct QuasarSVM harness to `quasar-test` (`#[quasar_test]`
  fixtures, `crate::cpi` instruction builders, `Outcome` assertions, typed
  `test.read::<UserAccount>` state checks). The `quasar-svm`,
  `spl-token-interface`, and `solana-program-pack` dev-dependencies are gone
  (libsecp256k1 stays for the Ethereum-signature path); compute-unit prints
  were dropped pending recalibration under 0.1.0.
- `Seed` is now imported from `quasar_lang::cpi`; 0.1.0 removed it from the
  prelude.

## 2026-07-07

Added this changelog. Changes prior to this date were tracked in git history only.
