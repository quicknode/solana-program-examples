# Changelog

## Unreleased

### Added

- **Curated asset registry.** A `Registry` account naming a curator authority separate from strategy managers, plus one `ApprovedAsset` account per approved mint (`["approved_asset", registry, mint]`), each binding the mint to its official Pyth price feed. Approval is checked by the account's existence. New instructions: `initialize_registry`, `approve_asset`.
- **Dynamic assets.** A strategy now grows its portfolio with `add_asset`, which registers an approved mint at the next index as an `AssetConfig` PDA (`["asset", strategy, index]`) and creates its vault. Assets occupy the contiguous range `0..asset_count`, up to `MAX_ASSETS` (16). Replaces the previous fixed two-asset layout.
- **Oracle-bounded slippage.** `deposit` and `rebalance` compute each swap's minimum output from the Pyth price and a strategy-level `max_slippage_bps` (capped at `MAX_SLIPPAGE_BPS` = 10%), instead of trusting a caller-supplied minimum. Set at creation via `initialize_strategy`.
- **Full-allocation invariant with immediate deployment.** A strategy accepts deposits only once its weights sum to exactly 10,000 bps (`deposit` reverts with `StrategyNotFullyAllocated` otherwise). `deposit` then swaps each depositor's USDC into the basket at its target weights through the registered router in the same transaction, so every deposit is fully invested (bar sub-cent rounding dust) and the USDC vault holds no idle cash.
- **Retirable assets.** `set_weight(weight_bps)` changes an asset's target weight after creation, including setting it to zero to retire it (reassign that weight to another asset to reach 100% and reopen deposits; `rebalance` liquidates the retired holdings). The asset's index is preserved, so the `0..asset_count` range the valuation handlers depend on stays contiguous.

### Changed

- `initialize_strategy` now takes `(index, fee_bps, max_slippage_bps, swap_router)` and binds the strategy to a registry; the strategy PDA is seeded by a caller-chosen index (`["strategy", index]`) rather than the manager's key, with the manager kept as a stored field. Weights and price feeds move to `add_asset`.
- `deposit` takes each asset's `[asset_config, vault, mint, rate, price_feed]` plus the router accounts, validates the complete `0..asset_count` set for NAV, requires the strategy to be fully allocated, and deploys the deposit at the target weights.
- `withdraw` takes each asset's `[asset_config, vault, mint, user_token_account]` and pays out every asset in kind over the complete `0..asset_count` set.
- `rebalance` takes `(sell_amount, usdc_to_invest)`; per-call minimums are gone.

### Fixed

- Boxed the `mock-swap-router` swap account structs, which overflowed the 4096-byte SBF stack frame under current platform-tools.
- Documented the per-manifest build (the workspace build strips the router entrypoint via feature unification).
