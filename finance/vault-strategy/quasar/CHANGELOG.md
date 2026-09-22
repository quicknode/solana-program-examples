# Changelog

## [2026-09-22]

### Changed

- Reject Pyth prices from before a cluster restart. Under Alpenglow the
  Clock's `unix_timestamp` may advance by at most twice the slot time elapsed
  since the parent block, so after a halt it trails real time and the
  60-second `publish_time` check would accept a price published just before
  the halt. `load_price` now reads the update's `posted_slot` (offset 125) and
  requires it to be after the `LastRestartSlot` sysvar's slot
  (`PricePredatesRestart`). quasar-lang has no LastRestartSlot sysvar, so
  `src/last_restart.rs` declares the layout and reads it via
  `sol_get_sysvar`. Tested by `deposit_rejects_price_from_before_a_restart`.

## [2026-09-10]

### Changed

- The mock swap router signs as its config account. The router used a second,
  dataless PDA as the owner of its USDC treasury and the mint authority of the
  asset mints. That PDA, its `Seeds` struct, and its seed constant are removed:
  the `RouterConfig` account (`["router_config"]`) now owns the treasury, is
  the mint authority of the asset mints, and signs the router's `mint_to` and
  `transfer_checked` CPIs with its own seeds and stored bump, the way the
  Strategy PDA signs for the share mint and vaults. The extra account is
  dropped from `set_rate`, both swap instructions, the vault's `deposit` and
  `rebalance` contexts, and the hand-built router CPIs, which now pass nine
  accounts instead of ten. Both test suites give the asset mint's mint
  authority to the router config account.

## [2026-07-22]

### Changed

- Migrated both programs (`vault-strategy` and `mock-swap-router`) to Quasar
  0.1.0 (`0.1.0-release` branch, rev `be60fca`): Quasar.toml rewritten to the
  0.1.0 schema, `idl-build` feature and `lib` crate-type added, and tests
  rewritten from the direct QuasarSVM harness to `quasar-test`
  (`#[quasar_test]` fixtures, `crate::cpi` instruction builders — including
  `remaining_accounts` for the per-asset deposit accounts — and `Outcome`
  assertions). The two-program deposit test loads the sibling router's
  compiled `.so` at runtime via
  `test.add(Program::new(ROUTER_ID, &std::fs::read("../mock-swap-router/target/deploy/quasar_mock_swap_router.so")...))`,
  so `quasar build` must run in `mock-swap-router` before the vault-strategy
  tests execute. The `quasar-svm` git dev-dependency is gone; compute-unit
  assertions were dropped pending recalibration under 0.1.0. Program-source
  fix for 0.1.0 in both programs: `Seed` is no longer in the prelude, so the
  instruction files that build signer seeds now import it from
  `quasar_lang::cpi`.

## 2026-07-20

- **`WhitelistEntry` renamed `ApprovedAsset`** (and `whitelist_asset` renamed `approve_asset`, PDA seed `"whitelist"` renamed `"approved_asset"`), naming the account after what it is: one curator-approved asset bound to its official price feed. The unused `AssetNotWhitelisted` error is removed; approval is checked by the `ApprovedAsset` account's existence. Doc comments and README now state that the `Registry` account is the curator record at the root of the approved set, not the list itself.

## 2026-07-07

Added this changelog. Changes prior to this date were tracked in git history only.
