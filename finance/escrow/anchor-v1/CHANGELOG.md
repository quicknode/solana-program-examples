# Changelog

## Unreleased, 2026-10-05

- The tests tell the book's story: token A is TSLAx, minted at 8 decimals, token B is USDC at 6, and Alice offers 1 TSLAx (`TSLAX_OFFERED`, 100,000,000 minor units) for 1,000 USDC (`USDC_WANTED`, 1,000,000,000 minor units). `test_cancel_offer_rejects_non_maker` asserts Anchor's `ConstraintHasOne` (2001), so every refusal test asserts its error code.

## 2026-10-04

- `take_offer` takes two arguments the taker signs, `minimum_token_a_out` and `maximum_token_b_in`, and refuses the take with `OfferTermsChanged` before any token moves if the vault holds less token A or the offer wants more token B than those bounds. An offer's address is its maker and `id`, so a maker could cancel an offer and re-make the same `id` at worse terms while a taker's transaction was in flight, and the transaction would trade at the new terms. `test_take_offer_rejects_switched_offer` and `test_take_offer_rejects_switched_offer_wanting_more_token_b` run that switch.

## 2026-09-29

- `make_offer` refuses an offer with zero tokens on either side, with `ZeroAmount`. An offer of a token for a different amount of itself was already refused before the handler runs, because the maker's token-A and token-B accounts would be the same account and Anchor refuses the same mutable account twice (`ConstraintDuplicateMutableAccount`); a test now pins that.

## 2026-07-07

Added this changelog. Changes prior to this date were tracked in git history only.
