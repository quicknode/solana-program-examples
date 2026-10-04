# Changelog

## Unreleased, 2026-10-04

The quasar-test suite asserts the error code of every refusal (`fails_with`
for the program's and Quasar's own errors, `fails` for the runtime's) instead
of only that the transaction failed. New test
`reclaim_collateral_after_expiry_returns_the_strike_to_the_put_writer`
follows the put from purchase to expiry and reclaim.
`writer_cannot_buy_their_own_option` asserts `AccountBorrowFailed`, the
refusal Quasar's account parsing gives the duplicate `buyer` and `writer`
slots. The purchase tests count the token program invocations in the logs, so
they show the token transfers a buy makes: two with a fee, one without. The
Kani crate gains `proof_collect_fees_pays_only_the_fees_owed`.

## 2026-10-03

The option now stores `underlying_amount` and `strike_amount`, the two amounts
that change hands, instead of `contracts`, `underlying_per_contract` and
`strike_per_contract`, which the program only ever multiplied together.
`write_option` takes the two amounts as arguments in place of the three
quantities. Settlement does no arithmetic: the collateral is one stored amount
and the exercise payment is the other. The write-time overflow check, and the
test that wrote an option whose collateral overflowed, are gone. The account
layout changes.

## 2026-09-30

Rename the market's `underlying_locked` and `quote_locked` to `underlying_owed`
and `quote_owed`. Each counts what the vault owes, to writers as collateral and
to writers and holders as settlement proceeds waiting to be collected, and
"locked" did not say to whom. The Quasar helpers `add_locked` and `sub_locked` are now `add_owed` and
`sub_owed`. The account layout is unchanged.

## 2026-09-10

The `Market` account is now the token authority of both vaults and signs
every transfer out of them with its own seeds. The separate dataless signer
PDA, the second bump `Market` stored for it, and the extra account
`initialize_market`, `cancel_option`, `exercise_option`, `collect_proceeds`,
`reclaim_collateral` and `collect_fees` took for it are gone.

## 2026-09-04

Initial version: Quasar port of the options example, matching the Anchor
sibling's design and math. `kind` and `status` are `u8` constants, the
writer's premium account is bound by owner and mint in `buy_option`, and
every token account must exist before it is used.
