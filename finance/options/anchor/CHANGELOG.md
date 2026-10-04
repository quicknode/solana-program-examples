# Changelog

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
