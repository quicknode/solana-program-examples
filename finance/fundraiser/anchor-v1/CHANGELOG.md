# Changelog

## 2026-10-01

### Fixed

- **A contributor account could outlive its fundraiser and count toward the next one.** `check_contributions` closed the Fundraiser account while the Contributor accounts, derived from its address, stayed open. The maker could then initialize a new fundraiser at the same address, and a leftover Contributor account would count as a contribution to it: `refund` would pay its old amount out of the new contributors' tokens. `check_contributions` now pays out the vault and sets a new `claimed` flag instead of closing anything, and the Fundraiser keeps a new `open_contributor_accounts` count. `close_fundraiser` closes a claimed fundraiser once that count is zero (else the new `ContributorAccountsOpen` error), so no Contributor account survives into the next raise. `test_stale_contributor_account_cannot_refund_from_next_raise`, `test_reinitialize_with_open_contributor_accounts_fails` and `test_close_fundraiser_with_open_contributor_accounts_fails` cover it.

### Changed

- `close_contributor` requires the fundraiser to be claimed (`FundraiserNotClaimed`, replacing `FundraiserStillOpen`) rather than gone, and decrements `open_contributor_accounts`.
- `refund` and `close_contributor` no longer require the contributor's signature. The tokens and rent still go only to the contributor, and anyone can send either, so the maker can refund or close every Contributor account without waiting on any contributor.
- `contribute` and `check_contributions` refuse a claimed fundraiser with the new `FundraiserClaimed` error.
- Program errors are public (`pub use error::*`) so the tests assert each failure's specific error code.

### Removed

- The per-contributor cap (`MAX_CONTRIBUTION_PERCENTAGE`, `PERCENTAGE_SCALER`, and the `ContributionTooBig` and `MaximumContributionsReached` errors). It limited each wallet, and a wallet costs nothing to create, so it did not stop one person funding most of a raise.

## 2026-09-28

- **Renamed from Token Fundraiser to Fundraiser.** The example moved from `finance/token-fundraiser` to `finance/fundraiser`: contributors receive no token, only a refund if the target is missed, so "Token" described something the program does not do. The program, its accounts, its instruction handlers and its tests are unchanged.

## 2026-09-23

### Added

- `close_contributor`, ported from the Anchor v2 copy: a contributor closes
  their Contributor account once the fundraiser is gone, taking back its rent.
  A successful raise closed the vault and the Fundraiser account but left every
  Contributor account open, and `refund`, their only other closer, runs only on
  a failed raise, so the rent was stuck. The handler's one check is that the
  passed fundraiser account is not owned by this program, else the new
  `FundraiserStillOpen` error. Two tests cover the rent returning after a claim
  and the refusal while the fundraiser exists.

## 2026-07-07

Added this changelog. Changes prior to this date were tracked in git history only.
