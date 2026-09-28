# Changelog

## 2026-09-28

- **Renamed from Token Fundraiser to Fundraiser.** The example moved from `finance/token-fundraiser` to `finance/fundraiser`: contributors receive no token, only a refund if the target is missed, so "Token" described something the program does not do. The program, its accounts, its instruction handlers and its tests are unchanged.

## 2026-09-14

### Added

- `close_contributor`: a contributor closes their Contributor account once the
  fundraiser is gone, taking back its rent. A successful raise closed the vault
  and the Fundraiser account but left every Contributor account open, and
  `refund`, their only other closer, runs only on a failed raise, so the rent
  was stuck. The handler's one check is that the passed fundraiser account is
  not owned by this program, else the new `FundraiserStillOpen` error. Two
  tests cover the rent returning after a claim and the refusal while the
  fundraiser exists.

## 2026-07-07

Added this changelog. Changes prior to this date were tracked in git history only.
