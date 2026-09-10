# Changelog

## 2026-09-10

Remove the separate dataless signing PDA (seeds `["authority", pool]`) that
owned the vault and the LP mint, and the bump field on `Pool` that recorded it.
The pool account is already a PDA, so it is now the custody vault's owner and
the LP mint's authority itself, and signs vault transfers and mint/burn CPIs
with its own seeds, `["pool", collateral_mint, oracle_feed, bump]`: the pattern
the escrow example uses for its vault and the vault-strategy example uses for
its share mint. `initialize_pool`, `add_liquidity`, `remove_liquidity`,
`close_position`, `liquidate_position` and `collect_fees` each take one account
fewer. The admin signer stored on `Pool` as `authority` (the pool operator) is
unchanged. `test_initialize_pool` now also checks that the vault's owner and the
mint's authority are the pool.

## 2026-09-08

Move to Anchor 1.2.0, LiteSVM 0.16.0 and solana-kite 0.5.0. No program source
changed. Two tests (`test_stale_price_rejected`,
`test_funding_charged_to_long`) used to warp to an absolute slot; LiteSVM now
starts its clock at a mainnet-like slot rather than zero, so those warps moved
time backwards and the price never went stale and no funding accrued. They now
warp relative to the current slot, as the other tests already did.

## 2026-08-14

Add `set_funding_rate`, so the pool operator can retune `funding_rate_per_slot`
after the pool is created. The rate is quoted per slot, so what a position costs
per hour depends on the cluster's slot time as well as on the rate; Solana lowers
the slot time over time, and a pool created before a reduction charges the
heavier side more per hour than it was set up to. The handler advances the
funding index at the old rate before storing the new one, so slots already
elapsed are charged at the rate that was in force for them. Tested by
`test_set_funding_rate_settles_at_the_old_rate_first` and
`test_only_authority_can_set_funding_rate`.

Also drop the "at 400ms/slot" gloss from the price-staleness constant: the
window is counted in slots on purpose, and what it comes to in seconds follows
the cluster.

## 2026-08-04

Reject oracle prices from before a cluster restart. A halt stops the slot
count but not the wall clock, so after a restart a feed can look fresh in
slots while its price is hours old; with leverage that error is amplified
market-wide. `read_oracle_price` now also requires the feed's slot to be
after the `LastRestartSlot` sysvar's slot (`PricePredatesRestart`). Tested
by `test_open_rejects_price_from_before_a_restart`.

## 2026-07-07

Added this changelog. Changes prior to this date were tracked in git history only.
