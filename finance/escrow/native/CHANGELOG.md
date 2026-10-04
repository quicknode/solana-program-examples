# Changelog

## 2026-10-04

- The `TakeOffer` instruction carries two fields the taker signs, `minimum_token_a_out` and `maximum_token_b_in`, and the handler refuses the take with `OfferTermsChanged` (appended to `EscrowError`, so existing codes keep their numbers) before any token moves if the vault holds less token A or the offer wants more token B than those bounds. An offer's address is its maker and `id`, so a maker could cancel an offer and re-make the same `id` at worse terms while a taker's transaction was in flight, and the transaction would trade at the new terms. `test_take_offer_rejects_switched_offer` and `test_take_offer_rejects_switched_offer_wanting_more_token_b` run that switch.

## 2026-09-29

- `make_offer` refuses an offer with zero tokens on either side (`ZeroAmount`) and an offer of a token for a different amount of itself (`SameMint`).

## 2026-07-07

Added this changelog. Changes prior to this date were tracked in git history only.
