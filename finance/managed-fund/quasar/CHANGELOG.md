# Changelog

## [Unreleased] - 2026-10-05

### Changed

- The management fee rounds up. `collect_fees` mints
  `ceil(total_shares × fee_bps × elapsed / (10_000 × SECONDS_PER_YEAR))`
  shares to the manager, so a fraction of a share owed is minted as a whole
  share and the rounding dilutes the holders. New `test_collect_fees` checks a
  year's fee on 1,000,000,000 shares is 10,000,000, exactly, and
  `test_collect_fees_rounds_up` that a day's fee, 27,397.26 shares, mints
  27,398.
- The two-asset tests mint TSLAx and NVDAx with eight decimals, as the real
  tokens have, and USDC with six (`ASSET_DECIMALS`, `USDC_DECIMALS`, and
  `SINGLE_ASSET_DECIMALS` for the single-asset fund's asset, replacing
  `DECIMALS`). Every asserted basket amount is in eight-decimal minor units
  and unchanged in major units. `test_valuation_scales_by_decimals_and_exponent`
  keeps running the story with an eight-decimal TSLAx on an exponent −5 feed,
  as the book describes, so it differs from the story in exponent only, and
  the new `test_valuation_scales_by_nine_decimals_and_exponent` runs it with
  TSLAx at nine decimals on the same feed, so the decimals vary as well; both
  get the story's share counts.

### Fixed

- A partially verified Pyth update is refused. `load_price` read the price at
  fixed offsets (price at 73) that assume the one-byte encoding of
  `verification_level`, `Full`. A `Partial { num_signatures }` update encodes
  it in two bytes, so every later field would be read a byte off. `load_price`
  now requires the tag at offset 40 to be `Full` (1) and fails with the new
  `PriceNotFullyVerified` error otherwise. The test feeds now carry the `Full`
  tag. Tested by `test_partially_verified_price_rejected`.

## [2026-10-03]

### Fixed

- Prices with a wide confidence interval are rejected. `load_price` reads the
  Pyth `conf` field (offset 81) and fails with the new
  `OracleConfidenceTooWide` error when it exceeds `MAX_CONFIDENCE_BPS` (100
  bps, 1% of the price). Deposit and rebalance are refused; withdraw reads no
  price and still pays out in kind. Tested by
  `test_wide_confidence_price_rejected`.

## [2026-10-01]

### Fixed

- Valuation scales by each asset's decimals and each Pyth feed's exponent. It
  assumed six decimals and an exponent of −8, so an eight-decimal asset was
  valued 100 times too high. `load_price` reads the exponent at offset 89,
  `AssetConfig` records `decimals` and `Fund` records `usdc_decimals`, and
  `deposit` and `rebalance` use `asset_value_in_usdc` and
  `usdc_to_asset_amount`. The mock router's `usdc_per_token` is now USDC minor
  units per whole token. Tested by
  `test_valuation_scales_by_decimals_and_exponent`.

### Changed

- `rebalance(sell_index, buy_index)` is permissionless and sizes its own trade
  from oracle prices and target weights, once the asset sold is above its
  target by the fund's `rebalance_threshold_bps`, a new `initialize_fund`
  argument bounded to 100..=2,000 with no setter. New errors
  `RebalanceThresholdOutOfRange`, `DriftBelowThreshold` and `NotUnderweight`.
  Before, the manager chose both legs' amounts and could churn a balanced fund
  through slippage. Tested by `test_rebalance_cannot_churn` and the other
  `test_rebalance_*` tests.

## [2026-09-28]

### Changed

- Renamed from Vault Strategy to Managed Fund. The example moved from
  `finance/vault-strategy` to `finance/managed-fund`. The crate is
  `quasar-managed-fund`, the `Strategy` account is `Fund`,
  `initialize_strategy` is `initialize_fund`, the `StrategyNotFullyAllocated`
  error is `FundNotFullyAllocated`, and the fund PDA's seed is `"fund"` instead
  of `"strategy"`, so fund addresses change. The program ID and the program's
  behavior are unchanged.

## [2026-09-22]

### Changed

- Ignore donations. The strategy records what it holds (`usdc_holdings`, and
  `asset_holdings` as little-endian u64s) and prices shares and pays
  withdrawals from those records, not from the vaults' token balances;
  `deposit`, `withdraw` and `rebalance` update them with what each transfer
  actually moved. Tokens transferred straight into a vault cannot inflate the
  share price, and `rebalance` cannot sell or spend them
  (`InsufficientHoldings`). Tested by `donation_does_not_inflate_share_price`.
- Reject a deposit leg that buys nothing (`DepositTooSmall`), which would mint
  shares against a fund worth nothing. Tested by
  `deposit_rejects_leg_that_buys_nothing`.
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
