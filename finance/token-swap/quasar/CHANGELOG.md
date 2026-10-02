# Changelog

## [2026-10-02] - Reserves at PDAs

### Changed

- `pool_a` and `pool_b` are PDAs of the pool, at seeds `[b"pool_a",
  pool_config]` and `[b"pool_b", pool_config]` (`PoolAPda`, `PoolBPda`), where
  before they were token accounts at addresses the client chose.
  `initialize_pool` creates them there with `init` rather than
  `init(idempotent)`, so a client finds a pool's reserves from the pool's
  address alone, and the instruction builder derives them. `PoolConfig` still
  records both addresses and every handler still checks them with `has_one`.

## [2026-10-02]

### Fixed

- `deposit_liquidity`, `withdraw_liquidity`, `swap_tokens` and
  `claim_admin_fees` took `pool_a` and `pool_b` with no check that they were
  the pool's reserves. A swap could name any mint-A token account the trader
  owned as `pool_a`: the handler priced the trade from that account's balance,
  sent the input into it, and paid out of the real `pool_b`. `PoolConfig` now
  records both reserve addresses at `initialize_pool`, and the four handlers
  check them with `has_one(pool_a)` and `has_one(pool_b)`, failing with the new
  `InvalidPoolVault` error. `swap_rejects_substituted_pool_vault` and
  `deposit_rejects_substituted_pool_vaults` run the attack. `PoolConfig` grows
  by 64 bytes, so pools created before this change cannot be read by it.
- `swap_tokens` re-checked the constant-product invariant against reserves it
  computed from the amounts it meant to transfer. It now reads both vaults'
  balances after the transfers land, as the Anchor versions do.

## [2026-09-22]

### Fixed

- `deposit_liquidity` now mints later deposits against the LP supply plus
  `MINIMUM_LIQUIDITY`, the divisor `withdraw_liquidity` already used. Dividing
  by the bare supply minted every depositor slightly less than they could
  redeem, let a donation as large as a victim's deposit round that deposit down
  to zero LP tokens, and left a pool whose LP tokens were all burned unable to
  take another deposit.

## [2026-09-10]

### Changed

- Removed the separate dataless signer PDA (seeds
  `[b"authority", config, mint_a, mint_b]`, with its `#[derive(Seeds)]` marker
  and seed constant) that owned the reserves and the LP mint. The `PoolConfig`
  account now owns both reserves, is the LP mint's mint authority, and signs
  the transfers out of the reserves and the LP mint with its own seeds
  `[config, mint_a, mint_b, bump]`, the way the escrow example's `offer`
  account signs for its vault. The extra signer account is gone from every
  instruction.

## [2026-07-22]

### Changed

- Migrated to Quasar 0.1.0 (`0.1.0-release` branch, rev `be60fca`): Quasar.toml
  rewritten to the 0.1.0 schema, `idl-build` feature and `lib` crate-type added,
  and tests rewritten from the direct QuasarSVM harness to `quasar-test`
  (`#[quasar_test]` fixtures, `crate::cpi` instruction builders, `Outcome`
  assertions). The AMM-math helpers (`mul_div`, `expected_swap_output`) and
  every scenario — deposits with the ratio-clamp regression pair, withdrawals,
  swaps, and all slippage guards — carry over verbatim. The `quasar-svm` git
  dev-dependency is gone; compute-unit assertions were dropped pending
  recalibration under 0.1.0. Program-source fix for 0.1.0: `Seed` is no longer
  in the prelude, so the instruction files that build signer seeds now import
  it from `quasar_lang::cpi`.

## 2026-07-07

Added this changelog. Changes prior to this date were tracked in git history only.
