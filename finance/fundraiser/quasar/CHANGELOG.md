# Changelog

## [2026-10-01]

### Fixed

- A Contributor account could outlive its fundraiser and count toward the
  next one. `check_contributions` closed the Fundraiser account while the
  Contributor accounts derived from its address stayed open, so the maker
  could initialize a new fundraiser at the same address and `refund` would pay
  a leftover account's old amount out of the new contributors' tokens.
  `check_contributions` now sets a new `claimed` flag and leaves the Fundraiser
  and vault open, and the Fundraiser counts `open_contributor_accounts`.

### Added

- `close_fundraiser` (discriminator 5): closes the vault and the Fundraiser
  once no Contributor account is open (`ContributorAccountsOpen`), and on an
  unclaimed fundraiser only after the deadline, with the target missed and
  every contribution refunded (`FundraiserNotEnded`, `TargetMet`, the new
  `RefundsOutstanding`). Any tokens left in the vault go to the maker.
- `FundraiserClaimed`: `contribute` and `check_contributions` refuse a claimed
  fundraiser.

### Changed

- `close_contributor` requires the fundraiser to be claimed
  (`FundraiserNotClaimed`, replacing `FundraiserStillOpen`).
- `refund` and `close_contributor` no longer require the contributor's
  signature, so the maker can close every Contributor account. `refund`
  checks that the destination token account belongs to the contributor.
  `quasar build` reports P006 (instruction missing signer) for both; that is
  intended.

## [2026-09-28]

### Changed

- Renamed from Token Fundraiser to Fundraiser. The example moved from
  `finance/token-fundraiser` to `finance/fundraiser`, and the crate is now
  `quasar-fundraiser`. The program's behavior is unchanged.

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
