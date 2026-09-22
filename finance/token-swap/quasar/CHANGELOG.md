# Changelog

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
