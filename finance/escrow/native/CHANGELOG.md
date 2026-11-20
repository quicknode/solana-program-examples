# Changelog

## Unreleased (2026-11-10)

- Tests run the three wrong-account attacks on `take_offer`, and each asserts that no token moves. `test_take_offer_rejects_taker_account_as_maker_token_account_b`: the taker names their own TSLAx account as the maker's, refused with `TokenAccountMismatch` because it is not the maker's associated token account for mint B. `test_take_offer_rejects_vault_of_another_offer`: with two of the maker's offers open, the taker takes the first and names the second's vault, refused with `TokenAccountMismatch` because it is not the first offer's associated token account for mint A. `test_take_offer_rejects_worthless_token_mint_b`: the taker pays in a mint they created, refused with `MintMismatch` against the offer's stored mint.

## Unreleased, 2026-10-05

- The tests tell the book's story the way the book does: mint A is USDC, minted at 6 decimals, mint B is TSLAx at 8, and the maker offers 250 USDC (`USDC_OFFERED`, 250,000,000 minor units) for 1 TSLAx (`TSLAX_WANTED`, 100,000,000 minor units), which the taker takes. Both start with the standard wallet of 1 SOL and 1,000 USDC, and the taker also holds 1 TSLAx, so the take leaves the taker at 1,250 USDC. The switched-offer tests re-make the offer at 1 USDC for the same 1 TSLAx, and at the same 250 USDC for 2 TSLAx. The take test asserts both sides' balances in both tokens. `test_cancel_offer_rejects_non_maker` asserts `MakerMismatch`, so every refusal test asserts its error code.

## 2026-10-04

- The `TakeOffer` instruction carries two fields the taker signs, `minimum_token_a_out` and `maximum_token_b_in`, and the handler refuses the take with `OfferTermsChanged` (appended to `EscrowError`, so existing codes keep their numbers) before any token moves if the vault holds less token A or the offer wants more token B than those bounds. An offer's address is its maker and `id`, so a maker could cancel an offer and re-make the same `id` at worse terms while a taker's transaction was in flight, and the transaction would trade at the new terms. `test_take_offer_rejects_switched_offer` and `test_take_offer_rejects_switched_offer_wanting_more_token_b` run that switch.

## 2026-09-29

- `make_offer` refuses an offer with zero tokens on either side (`ZeroAmount`) and an offer of a token for a different amount of itself (`SameMint`).

## 2026-07-07

Added this changelog. Changes prior to this date were tracked in git history only.
