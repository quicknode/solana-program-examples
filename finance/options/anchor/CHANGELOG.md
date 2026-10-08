# Changelog

## Unreleased, 2026-10-07

`buy_option` takes an `OptionTerms` argument, the terms the buyer read from
the option, and refuses the purchase with the new error `OptionTermsChanged`
unless the option still has exactly those terms (kind, `underlying_amount`,
`strike_amount`, `premium` and `expiry`). Without it a writer could cancel an
option and write a new one at the same address (the same `id`) at a higher
premium, on fewer shares or with a sooner expiry while a purchase was on its
way, the switched-offer attack the escrow's `take_offer` already refuses. New
tests `test_buy_option_refuses_a_switched_option` (four switches, each
refused with nothing moved) and `test_buy_option_succeeds_when_the_terms_match`.
The suite's `buy_option` helper passes the option's current terms, and
`buy_option_with_terms` passes the terms a buyer saw earlier.

`write_option`, `cancel_option` and `reclaim_collateral` create the writer's
underlying associated token account if it does not exist, at the writer's
expense, as `collect_proceeds` already did; `cancel_option` and
`reclaim_collateral` take the associated token and system programs for it.
A put writer who has never held the underlying could not write, cancel or
reclaim before. For an account that exists, the mint and authority
constraints are unchanged. New tests
`test_put_writer_without_an_underlying_account_writes_and_reclaims` and
`test_put_writer_without_an_underlying_account_writes_and_cancels` start
Carol with no NVDAx account (the new `person_without_underlying_account`
helper) and pin her rent and token balances to the minor unit.

The `Market` doc comment names the `*_owed` fields instead of the old
`*_locked` names.

## Unreleased, 2026-10-05

The venue's fee rounds up: `split_premium` takes the ceiling of
`premium * fee_bps / 10_000` and pays the writer the premium minus the fee,
so the venue, not the writer, takes the rounding minor unit on a premium that
is not a multiple of the rate. The walkthrough's premiums (25 and 20 USDC at
1%) are multiples of the rate, so their fees (0.25 and 0.20 USDC) are the
same either way. New test `test_fee_rounds_up_and_the_writer_takes_the_remainder`
buys a 10.000001 USDC option and checks a fee of 0.100001 USDC and 9.90 USDC
to the writer. The Kani harness `proof_premium_split_conserves_the_premium`
checks the exact ceiling, and its unit tests the rounded cases, including a
one-minor-unit premium that rounds entirely into the fee at the highest rate.
NVDAx is minted with 8 decimals in the suite, as the real token is, and USDC
with 6: 5 NVDAx is 500,000,000 minor units; every USDC amount is unchanged.

## Unreleased, 2026-10-04

The LiteSVM suite asserts the error code of every refusal
(`assert_fails_with` for the program's errors, `assert_fails_with_anchor_error`
for Anchor's constraint errors) instead of only that the transaction failed;
the `errors` module is public so it can. Two refusals that had been passing as
duplicate transactions, the second fee sweep and a stranger's second exercise
attempt, now reach the handler on a fresh blockhash and fail for the rule under
test. New test
`test_reclaim_collateral_after_expiry_returns_the_strike_to_the_put_writer`
follows the put from purchase to expiry and reclaim.
`test_writer_cannot_buy_their_own_option` asserts
`ConstraintDuplicateMutableAccount`, the Anchor check that refuses the
purchase. The suite sends transactions through LiteSVM directly and reads the
metadata back, so the purchase tests count the token transfers a buy makes:
two with a fee, one without. The Kani crate gains
`proof_collect_fees_pays_only_the_fees_owed`.

## 2026-10-03

An option now stores `underlying_amount` and `strike_amount`, the two amounts
that change hands on exercise, instead of `contracts`, `underlying_per_contract`
and `strike_per_contract`, which the program only ever multiplied together. The
same change applies to the `OptionTerms` that `write_option` takes. Settlement
does no arithmetic: the collateral, the exercise payment and the proceeds are
each one of the two stored amounts, and `contract_math` loses `underlying_total`
and `strike_total`. With no multiplication left, nothing can overflow at write
time, so the test that wrote a lot whose collateral overflowed is gone. The
option account is 8 bytes smaller.

## 2026-09-30

Rename the market's `underlying_locked` and `quote_locked` to `underlying_owed`
and `quote_owed`. Each counts what the vault owes, to writers as collateral and
to writers and holders as settlement proceeds waiting to be collected, and
"locked" did not say to whom. The account layout is unchanged.

## 2026-09-10

The `Market` account is now the token authority of both vaults and signs
every transfer out of them with its own seeds. The separate dataless signer
PDA, the second bump `Market` stored for it, and the extra account
`initialize_market`, `cancel_option`, `exercise_option`, `collect_proceeds`,
`reclaim_collateral` and `collect_fees` took for it are gone.

## 2026-09-04

Initial version: a fully collateralized, physically settled options venue.
A writer posts the whole obligation (the underlying for a call, the strike
in the quote token for a put) and lists an option at a premium; a buyer pays the
premium and becomes the holder; the holder may exercise before expiry; after
expiry the writer reclaims the collateral. Eight instruction handlers
(`initialize_market`, `write_option`, `buy_option`, `cancel_option`,
`exercise_option`, `collect_proceeds`, `reclaim_collateral`,
`collect_fees`), a custody ledger on the market account asserted after every
transfer, and a LiteSVM suite covering both kinds and all three exits.
