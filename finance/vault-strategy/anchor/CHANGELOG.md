# Changelog

## Unreleased

### Added

- **Curated asset registry.** A `Registry` plus per-mint `WhitelistEntry` accounts, maintained by a protocol authority separate from strategy managers. Each entry binds an approved mint to its official Pyth price feed. New instructions: `initialize_registry`, `whitelist_asset`.
- **Dynamic assets.** A strategy now grows its portfolio with `add_asset`, which registers a whitelisted mint at the next index as an `AssetConfig` PDA (`["asset", strategy, index]`) and creates its vault. Assets occupy the contiguous range `0..asset_count`, up to `MAX_ASSETS` (8). Replaces the previous fixed two-asset layout.
- **Oracle-bounded slippage.** `invest` and `rebalance` now compute each swap's minimum output from the Pyth price and a strategy-level `max_slippage_bps` (capped at `MAX_SLIPPAGE_BPS` = 10%), instead of trusting a caller-supplied minimum. Set at creation via `initialize_strategy`.

### Changed

- `initialize_strategy` now takes `(fee_bps, max_slippage_bps, swap_router)` and binds the strategy to a registry; weights and price feeds move to `add_asset`.
- `deposit` and `withdraw` take each asset's accounts as remaining accounts and validate the complete `0..asset_count` set, so NAV and in-kind payouts always cover every asset.
- `invest` takes `(usdc_amount)` and `rebalance` takes `(sell_amount, usdc_to_invest)`; per-call minimums are gone.

### Fixed

- Boxed the `mock-swap-router` swap account structs, which overflowed the 4096-byte SBF stack frame under current platform-tools.
- Documented the per-manifest build (the workspace build strips the router entrypoint via feature unification).
