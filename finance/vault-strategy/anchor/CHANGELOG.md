# Changelog

## Unreleased

### Added

- **Curated asset registry.** A `Registry` plus per-mint `WhitelistEntry` accounts, maintained by a protocol authority separate from strategy managers. Each entry binds an approved mint to its official Pyth price feed. New instructions: `initialize_registry`, `whitelist_asset`.
- **Dynamic assets.** A strategy now grows its portfolio with `add_asset`, which registers a whitelisted mint at the next index as an `AssetConfig` PDA (`["asset", strategy, index]`) and creates its vault. Assets occupy the contiguous range `0..asset_count`, up to `MAX_ASSETS` (8). Replaces the previous fixed two-asset layout.
- **Oracle-bounded slippage.** `deposit`, `invest`, and `rebalance` compute each swap's minimum output from the Pyth price and a strategy-level `max_slippage_bps` (capped at `MAX_SLIPPAGE_BPS` = 10%), instead of trusting a caller-supplied minimum. Set at creation via `initialize_strategy`.
- **Immediate deployment on deposit.** `deposit` swaps each depositor's USDC into the basket at its target weights through the registered router in the same transaction. The USDC vault no longer holds idle deposits; only the unallocated remainder (when the weights sum below 10,000) stays as USDC.
- **Retirable assets.** `set_weight(weight_bps)` changes an asset's target weight after creation, including setting it to zero to retire it. The asset's index is preserved, so the `0..asset_count` range the valuation handlers depend on stays contiguous.

### Changed

- `initialize_strategy` now takes `(index, fee_bps, max_slippage_bps, swap_router)` and binds the strategy to a registry; the strategy PDA is seeded by a caller-chosen index (`["strategy", index]`) rather than the manager's key, with the manager kept as a stored field. Weights and price feeds move to `add_asset`.
- `deposit` takes each asset's `[asset_config, vault, mint, rate, price_feed]` plus the router accounts, validates the complete `0..asset_count` set for NAV, and deploys the deposit at the target weights.
- `withdraw` takes each asset's `[asset_config, vault, mint, user_token_account]` and pays out every asset in kind over the complete `0..asset_count` set.
- `invest` takes `(usdc_amount)` and `rebalance` takes `(sell_amount, usdc_to_invest)`; per-call minimums are gone.

### Fixed

- Boxed the `mock-swap-router` swap account structs, which overflowed the 4096-byte SBF stack frame under current platform-tools.
- Documented the per-manifest build (the workspace build strips the router entrypoint via feature unification).
