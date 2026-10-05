# Changelog

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

The option now stores `underlying_amount` and `strike_amount`, the two amounts
that change hands on exercise, instead of `contracts`, `underlying_per_contract`
and `strike_per_contract`, which the program only ever multiplied together.
`OptionTerms` changes the same way. Settlement does no arithmetic: the
collateral, the exercise payment and the proceeds are each one of the stored
amounts. With no multiplication left there is nothing to overflow at write
time, so the write-time overflow test is gone.

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
