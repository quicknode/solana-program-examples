# Changelog

## Unreleased (2026-10-04)

`initialize_pool` now takes the creator's first deposit: it gains `amount_a`
and `amount_b` arguments and the `creator`, `creator_token_a`,
`creator_token_b` and `liquidity_provider_token` accounts, moves both amounts
into the reserves it creates, and mints the creator
`sqrt(amount_a * amount_b) - MINIMUM_LIQUIDITY` LP tokens. A zero on either
side fails with the new `EmptyInitialDeposit`. A pool created empty let
whoever deposited first set its price, and clamped the creator's own deposit
to that ratio. `deposit_liquidity` no longer has a pool-creation branch: it
refuses an empty effective reserve with `EmptyPoolReserve`, whose message now
reads "Pool reserves must both be positive to deposit or swap". The
square-root arithmetic (`initial_lp_amount`) and the transfers and LP mint
both handlers end with (`deposit_and_mint_lp_tokens`) live in the new
`liquidity` module, so there is one copy of each. New tests:
`test_initialize_pool_takes_first_deposit`,
`test_initialize_pool_rejects_zero_amount_a`,
`test_initialize_pool_rejects_zero_amount_b` and
`test_pool_creation_cannot_be_front_run`, which runs the front-run (a hostile
ratio deposited right after the pool opens is clamped to the creator's price)
and checks that a deposit against an empty reserve is refused; every other
test opens its pool through `initialize_pool`.

## 2026-09-22

`deposit_liquidity` now mints later deposits against the LP supply plus
`MINIMUM_LIQUIDITY`, the divisor `withdraw_liquidity` already used. Dividing by
the bare supply minted every depositor slightly less than they could redeem,
let a donation as large as a victim's deposit (rather than 101 times it) round
that deposit down to zero LP tokens, and left a pool whose LP tokens were all
burned unable to take another deposit. New tests cover the donation attack and
the deposit into an emptied pool.

## 2026-09-10

Removed the separate dataless signer PDA (seeds `[config, mint_a, mint_b,
b"authority"]`) that owned the reserves and the LP mint. The `PoolConfig`
account now owns both reserves (`pool_a` and `pool_b` are its associated token
accounts), is the LP mint's mint authority, and signs the transfers out of the
reserves and the LP mint/burn CPIs with its own seeds
`[config, mint_a, mint_b, bump]`, the way the escrow example's `offer` account
signs for its vault. The `b"authority"` seed constant and the extra signer
account are gone from every instruction, and the reserve addresses changed with
their owner: derive them as the associated token accounts of `pool_config`.

## 2026-07-07

Added this changelog. Changes prior to this date were tracked in git history only.
