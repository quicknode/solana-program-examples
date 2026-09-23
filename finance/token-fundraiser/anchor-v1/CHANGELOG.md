# Changelog

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
