# Changelog

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
