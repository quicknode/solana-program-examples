# Changelog

## 2026-09-22

Accrue funding by the wall clock instead of by slots. The rate was quoted per
slot, so what a position cost per hour moved with the cluster's slot time, and
the reduction to 200 ms slots doubled it. `Pool::funding_rate_per_slot` is now
`funding_rate_per_second`, and `last_funding_slot` is now
`last_funding_timestamp`, the Clock's `unix_timestamp` at the last accrual; the
same rename applies to `initialize_pool`'s and `set_funding_rate`'s arguments. A
timestamp at or before the stored one accrues nothing. `advance_funding` is
replaced by `accrue_funding`, which updates the pool in place as the Anchor
version's does. Tested by `funding_follows_seconds_not_slots`, with
`set_funding_rate_settles_at_the_old_rate_first` and
`inflating_liquidity_through_own_trades_does_not_pay` now counting seconds.

`add_liquidity` and `remove_liquidity` now divide by the share supply plus
`MINIMUM_LIQUIDITY`, so the 1,000 shares withheld from the first deposit belong
to nobody and their slice of the pool stays locked. Before, both divided by the
bare supply, and a provider who was also the only trader could pay funding into
`liquidity` to inflate their single share and take part of the next deposit. A
pool whose providers have all left now prices the next deposit against the
locked slice. `inflating_liquidity_through_own_trades_does_not_pay` runs the
attack.

## 2026-09-10

Remove the separate dataless signing PDA (seeds `["authority", pool]`) that
owned the vault and the LP mint, its seeds struct, and the bump field on `Pool`
that recorded it. The pool account is already a PDA, so it is now the custody
vault's owner and the LP mint's authority itself, and signs vault transfers and
mint/burn CPIs with its own seeds, `["pool", collateral_mint, oracle_feed,
bump]`: the pattern the escrow example uses for its vault and the vault-strategy
example uses for its share mint. `initialize_pool`, `add_liquidity`,
`remove_liquidity`, `close_position`, `liquidate_position` and `collect_fees`
each take one account fewer. The admin signer stored on `Pool` as `authority`
(the pool operator) is unchanged.
`initialize_pool_creates_pool_vault_and_lp_mint` now also checks that the
vault's owner and the mint's authority are the pool.

## 2026-08-14

Add `set_funding_rate` (discriminator 7), so the pool operator can retune
`funding_rate_per_slot` after the pool is created. The rate is quoted per slot,
so what a position costs per hour depends on the cluster's slot time as well as
on the rate; Solana lowers the slot time over time, and a pool created before a
reduction charges the heavier side more per hour than it was set up to. The
handler advances the funding index at the old rate before storing the new one,
so slots already elapsed are charged at the rate that was in force for them.
Tested by `set_funding_rate_settles_at_the_old_rate_first` and
`only_the_authority_can_set_the_funding_rate`.

Also drop the "at 400ms" gloss from the price-staleness constant: the window is
counted in slots on purpose, and what it comes to in seconds follows the
cluster.

## 2026-08-04

Reject oracle prices from before a cluster restart: `read_oracle_price`
requires the feed's slot to be after the `LastRestartSlot` sysvar's slot
(`PRICE_PREDATES_RESTART`). quasar-lang has no LastRestartSlot sysvar, so
`src/last_restart.rs` declares the layout and reads it via
`sol_get_sysvar`. Also pinned `zeropod = "=0.3.3"` (zeropod 0.3.4 moved to
wincode 0.5 while quasar-lang's pinned rev stays on wincode 0.4, so a fresh
resolve failed every Pod* trait bound). Tested by
`open_rejects_price_from_before_a_restart`.

## [2026-07-22]

### Changed

- Migrated to Quasar 0.1.0 (`0.1.0-release` branch, rev `be60fca`): Quasar.toml
  rewritten to the 0.1.0 schema, `idl-build` feature and `lib` crate-type added,
  and tests rewritten from the direct QuasarSVM harness to `quasar-test`
  (`#[quasar_test]` fixtures, `crate::cpi` instruction builders, `Outcome`
  assertions; the hand-crafted oracle feed account is injected via
  `test.set_account`). The `quasar-svm` git dev-dependency is gone. Program-
  source fix for 0.1.0: `Seed` is no longer in the prelude, so the instruction
  files that build signer seeds now import it from `quasar_lang::cpi`.

## 2026-07-07

Added this changelog. Changes prior to this date were tracked in git history only.
