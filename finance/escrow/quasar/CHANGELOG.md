# Changelog

## [Unreleased, 2026-10-05]

### Changed

- The tests tell the book's story: `TSLAX_MINT` is minted at 8 decimals,
  `USDC_MINT` at 6, and the maker offers 1 TSLAx (`TSLAX_OFFERED`,
  100,000,000 minor units) for 1,000 USDC (`USDC_WANTED`, 1,000,000,000 minor
  units). The substituted-mint, substituted-vault and non-maker-cancel tests
  assert `QuasarError::HasOneMismatch` (3005), so every refusal test asserts
  its error code.

## [2026-10-04]

### Fixed

- `take_offer` takes two arguments the taker signs, `minimum_token_a_out`
  and `maximum_token_b_in`, and refuses the take with `OfferTermsChanged`
  (error 6001) before any token moves if the vault holds less token A or the
  offer's `receive` is more than those bounds. An offer's address is its
  maker and `id`, so a maker could cancel an offer and re-make the same `id`
  at worse terms while a taker's transaction was in flight, and the
  transaction would trade at the new terms.
  `test_take_offer_rejects_switched_offer` and
  `test_take_offer_rejects_switched_offer_wanting_more_token_b` run that
  switch.

## [2026-09-29]

### Changed

- `make_offer` refuses an offer with zero tokens on either side (`ZeroAmount`).
  An offer of a token for a different amount of itself was already refused
  before the handler runs, because both mint slots would hold the same
  account and loading it twice fails with `AccountBorrowFailed`; a test now
  pins that.

## [2026-07-22]

### Changed

- Migrated to Quasar 0.1.0 (`0.1.0-release` branch, rev `be60fca`): Quasar.toml
  rewritten to the 0.1.0 schema, `idl-build` feature and `lib` crate-type added,
  and tests rewritten from the direct QuasarSVM harness to `quasar-test`
  (`#[quasar_test]` fixtures, `crate::cpi` instruction builders, `Outcome`
  assertions). The `quasar-svm` git dev-dependency is gone; compute-unit
  assertions were dropped pending recalibration under 0.1.0. Program-source
  fix for 0.1.0: `Seed` is no longer in the prelude, so `take_offer.rs` and
  `cancel_offer.rs` now import it from `quasar_lang::cpi`.

## 2026-07-07

Added this changelog. Changes prior to this date were tracked in git history only.
