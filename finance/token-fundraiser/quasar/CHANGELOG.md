# Changelog

## [2026-09-14]

### Added

- `close_contributor` (discriminator 4): a contributor closes their Contributor
  account once the fundraiser is gone, taking back its rent. A successful raise
  closed the vault and the Fundraiser account but left every Contributor
  account open, and `refund`, their only other closer, runs only on a failed
  raise, so the rent was stuck. The handler's one check is that the passed
  fundraiser account is not owned by this program, else the new
  `FundraiserStillOpen` error. Two tests cover the rent returning after a claim
  and the refusal while the fundraiser exists.

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
