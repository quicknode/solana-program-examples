# Changelog

## 2026-09-29

- `make_offer` refuses an offer with zero tokens on either side, with `ZeroAmount`. An offer of a token for a different amount of itself was already refused before the handler runs, because the maker's token-A and token-B accounts would be the same account and Anchor refuses the same mutable account twice (`ConstraintDuplicateMutableAccount`); a test now pins that.

## 2026-07-07

Added this changelog. Changes prior to this date were tracked in git history only.
