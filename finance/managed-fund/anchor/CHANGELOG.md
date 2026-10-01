# Changelog

## 2026-10-01

- **Valuation scales by each asset's decimals and each feed's exponent.** The fund valued an asset as `amount × price / 10⁸`, which is right only when the asset has USDC's 6 decimals and its Pyth feed has exponent −8, and nothing checked either. An eight-decimal asset would have been valued 100 times too high, so a later depositor would have bought almost no shares; Pyth's US equity feeds use exponent −5, a further factor of 1,000. `load_price` now reads the exponent (offset 89) and returns an `OraclePrice`, `AssetConfig` records the mint's `decimals` and `Fund` the USDC mint's `usdc_decimals`, and `deposit` and `rebalance` value and size swaps with `asset_value_in_usdc` and `usdc_to_asset_amount`, which scale by `10^(usdc_decimals + exponent − asset_decimals)`. `PYTH_PRICE_PRECISION` is removed. The mock router's `usdc_per_token` is now USDC minor units per whole token, and its swaps scale by the asset mint's decimals. Tested by `test_valuation_scales_by_decimals_and_exponent`, which runs the story with an eight-decimal TSLAx on an exponent −5 feed.
- **Anyone may rebalance, and the program sizes the trade.** `rebalance(sell_amount, usdc_to_invest)`, manager-only, is now `rebalance(sell_index, buy_index)`, signed by any `caller`, with the same five remaining accounts per asset as `deposit`. It values every asset and requires the asset sold to sit above its target by at least the fund's threshold (new `DriftBelowThreshold`; a retired asset may always be sold) and the asset bought to sit below its target (new `NotUnderweight`). It trades the smaller of the two gaps and spends only what the sale brought in. Before, the manager chose both amounts, so they could trade a balanced fund back and forth and bleed it through slippage, to the benefit of whoever filled the trades. A fund at its targets now has no trade to make. `initialize_fund` takes a `rebalance_threshold_bps`, between `MIN_REBALANCE_THRESHOLD_BPS` (100) and `MAX_REBALANCE_THRESHOLD_BPS` (2,000) (new `RebalanceThresholdOutOfRange`), stored on `Fund` with no setter. Tested by `test_rebalance_refuses_fund_at_target`, `test_rebalance_refuses_drift_below_threshold`, `test_rebalance_cannot_churn`, `test_rebalance_refuses_buying_overweight_asset`, `test_rebalance_rejects_incomplete_assets`, `test_rebalance_sells_retired_asset`, `test_rebalance_ignores_donations` (replacing `test_rebalance_cannot_spend_donated_usdc`) and `test_initialize_rejects_threshold_out_of_range`. The Kani proofs gain `proof_rebalance_trade_never_overshoots` and `proof_sell_amount_never_worth_more_than_trade`. The web app's IDL, instruction builders and rebalance panel follow.

## 2026-09-28

- **Renamed from Vault Strategy to Managed Fund.** The example moved from `finance/vault-strategy` to `finance/managed-fund` and now uses the finance term for the product. The program crate is `managed-fund` (library `managed_fund`), the `Strategy` account is `Fund`, `initialize_strategy` is `initialize_fund`, the `StrategyNotFullyAllocated` error is `FundNotFullyAllocated`, and the fund PDA's seed is `"fund"` instead of `"strategy"`, so fund addresses change. The `initialize_fund` instruction and `Fund` account have new Anchor discriminators, and the web app's IDL and client follow. `vault` still means only a single-asset token account the fund owns. The program ID and the program's behavior are unchanged.

## 2026-09-22

- **Donations are ignored.** The strategy records what it holds (`usdc_holdings` and `asset_holdings` on `Strategy`) and prices shares and pays withdrawals from those records, not from the vaults' token balances. `deposit`, `withdraw` and `rebalance` update them with what each transfer actually moved. Tokens transferred straight into a vault are outside the fund: they cannot inflate the share price, are never paid out, and `rebalance` can neither sell nor spend them (new `InsufficientHoldings` error). This closes the first-depositor inflation attack. Tested by `test_donation_does_not_inflate_share_price` and `test_rebalance_cannot_spend_donated_usdc`, and `test_full_lifecycle` checks the records against the vault balances after every step.
- **A deposit leg that buys nothing is rejected.** A deposit so small that a swap spends USDC and returns none of the asset now fails with `DepositTooSmall`; before, it minted shares against a fund worth nothing and every later deposit then divided by zero. Tested by `test_deposit_rejects_leg_that_buys_nothing`.
- The web app reads the recorded holdings for NAV and its allocation view, and its IDL gains the new fields and errors.
- **Prices from before a cluster restart are rejected.** Under Alpenglow each leader sets the Clock's `unix_timestamp`, which may advance by at most twice the slot time elapsed since the parent block, so after a halt the timestamp trails real time and catches up gradually. The 60-second `publish_time` check would therefore accept a Pyth price published just before a multi-hour halt. `load_price` now also reads the update's `posted_slot` (offset 125) and requires it to be after the `LastRestartSlot` sysvar's slot, failing with the new `PricePredatesRestart` error until Pyth posts again. Tested by `test_deposit_rejects_price_from_before_restart`; the web app's IDL gains the error.

## 2026-09-10

- **The mock swap router signs as its config account.** The router used a second, dataless PDA as the owner of its USDC treasury and the mint authority of the basket mints. That PDA is removed: `router_config` (`["router_config"]`) now owns the treasury, is the mint authority of the basket mints, and signs the router's `mint_to` and `transfer_checked` CPIs with its own seeds and stored bump, mirroring how the strategy PDA signs for the share mint and vaults. The extra account is gone from every router instruction, from the vault's `deposit` and `rebalance` contexts, and from the router CPI account lists; the treasury is now the `router_config` account's USDC associated token account. Tests and the web app's PDA helpers, instruction builders, and IDL follow.

## 2026-07-20

- **`WhitelistEntry` renamed `ApprovedAsset`** (and `whitelist_asset` renamed `approve_asset`, PDA seed `"whitelist"` renamed `"approved_asset"`), naming the account after what it is: one curator-approved asset bound to its official price feed. The unused `AssetNotWhitelisted` error is removed; approval is checked by the `ApprovedAsset` account's existence. Doc comments and README now state that the `Registry` account is the curator record at the root of the approved set, not the list itself.

## 2026-07-01

### Added

- **Curated asset registry.** A `Registry` plus per-mint `WhitelistEntry` accounts, maintained by a protocol authority separate from strategy managers. Each entry binds an approved mint to its official Pyth price feed. New instructions: `initialize_registry`, `whitelist_asset`.
- **Dynamic assets.** A strategy now grows its portfolio with `add_asset`, which registers a whitelisted mint at the next index as an `AssetConfig` PDA (`["asset", strategy, index]`) and creates its vault. Assets occupy the contiguous range `0..asset_count`, up to `MAX_ASSETS` (16). Replaces the previous fixed two-asset layout.
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
