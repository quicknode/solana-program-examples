# Solana Vault Strategy (Anchor)

A manager-run investment vault on Solana. Users deposit [USDC](https://www.investopedia.com/terms/u/usd-coin-usdc.asp) and receive shares representing proportional ownership of a portfolio of assets. The manager adds assets from a curated whitelist and sets their target weights; each deposit is deployed across those assets at its weights in the same transaction. The manager rebalances as prices drift, earns a fee, and depositors withdraw their proportional slice in kind when they choose.

The example uses two stocks as the portfolio assets: **TSLAx** (Tesla) and **NVDAx** (NVIDIA) - [xStocks](https://backed.fi/xstocks) issued on Solana by Backed Finance. In tests these are mock [tokens](https://solana.com/docs/terminology#token).

A note on the word **vault**: by the common standard (ERC-4626) a vault holds a single asset. Here a vault is one single-asset [token account](https://solana.com/docs/terminology#token-account), and the whole multi-asset construct is the **strategy**, which owns one vault per asset plus a USDC vault. So "vault strategy" reads literally: a strategy built from vaults.

---

## Programs

- **`vault-strategy`**: Registry/whitelist, strategy creation, asset registration, deposits, share minting, fee accrual, rebalancing, withdrawals
- **`mock-swap-router`**: Test-only fake Jupiter. Stores exchange rates, mints/burns basket tokens for USDC. Replaced by real [Jupiter](https://jup.ag) in production.

---

## Key Financial Concepts

### Net Asset Value (NAV)

[NAV](https://www.investopedia.com/terms/n/nav.asp) is the total value of everything the strategy holds: the USDC vault balance plus each asset vault balance valued at its Pyth price. It prices new deposits fairly, so every depositor pays the same per-share price regardless of when they join.

Because the asset set is dynamic, `deposit` must value *every* asset. The assets live at PDAs indexed `0..asset_count`, and `deposit` re-derives that complete range from the accounts it is given, refusing to run if any asset is missing (`IncompleteAssetAccounts`). This makes it structurally impossible to omit an asset and understate NAV.

Referencing every asset has a transaction-size cost: `deposit` pulls in `14 + 5N` accounts and `withdraw` `10 + 4N`, where `N` is the asset count. That stays within Solana's 128-account transaction lock limit at the `MAX_ASSETS` cap of 16 (94 accounts for `deposit`), but a basket beyond roughly three assets no longer fits a legacy transaction's 1232-byte limit, so the client must send a v0 transaction with an [Address Lookup Table](https://docs.anza.xyz/proposals/versioned-transactions).

Prices come from [Pyth Network](https://pyth.network/) `PriceUpdateV2` accounts. A 60-second staleness window is enforced; zero or negative prices are rejected.

### Shares

A [share](https://www.investopedia.com/terms/s/shares.asp) represents a fraction of the whole strategy. Hold 1% of shares and you own 1% of every vault.

- **First deposit**: shares are issued 1:1 with USDC minor units (initial price of 1 USDC per share).
- **Later deposits**: `shares_to_mint = deposit_usdc × total_shares / NAV`.
- Shares are [SPL tokens](https://solana.com/docs/terminology#token); the share mint's address is a [PDA](https://solana.com/docs/terminology#program-derived-address-pda), so it is deterministic and the strategy PDA is its mint authority.

### Management Fee

A [management fee](https://www.investopedia.com/terms/m/managementfee.asp), in [basis points](https://www.investopedia.com/terms/b/basispoint.asp) (100 bps = 1% per year), is charged by *minting new shares to the manager*, diluting holders proportionally. This is the common onchain pattern (Yearn, Lido charge fees this way) and differs from a traditional fund, which deducts the fee in cash from assets.

```
fee_shares = total_shares × fee_bps × elapsed_seconds / (10_000 × 31_536_000)
```

`collect_fees` is permissionless. The fee is fixed at creation and capped at `MAX_FEE_BPS` (1,000 bps = 10%); there is no setter to raise it later.

### Weights and Rebalancing

Each asset carries a target **weight** in basis points (e.g. 40% TSLAx, 60% NVDAx). A strategy accepts deposits only once its weights sum to exactly 10,000 (`add_asset` and `set_weight` keep the running sum at or below 10,000; `deposit` requires it to equal 10,000, else `StrategyNotFullyAllocated`). So a strategy is either still being configured or fully allocated and live, and `deposit` deploys each depositor's USDC straight into the basket at those weights, fully invested bar sub-cent rounding dust. There is no idle-cash mode.

[Rebalancing](https://www.investopedia.com/terms/r/rebalancing.asp) handles the drift that prices create after a deposit: `rebalance` sells an over-weight asset for USDC and buys an under-weight one in a single atomic instruction. `set_weight` changes a target after creation, including setting it to zero to **retire** an asset: deposits stop allocating to it, the manager sells its holdings out with `rebalance`, and the now-empty vault keeps its index so the contiguous `0..asset_count` range stays intact (the index is never reused).

### Slippage, bounded by the oracle

[Slippage](https://www.investopedia.com/terms/s/slippage.asp) is the gap between the expected and the realized amount of a swap. Rather than trust a manager-supplied minimum, `deposit` and `rebalance` compute the floor themselves from the Pyth price and the strategy's `max_slippage_bps`: a swap whose output falls more than that tolerance below the oracle-implied amount reverts. `max_slippage_bps` is set at creation and capped at `MAX_SLIPPAGE_BPS` (1,000 bps = 10%).

### In-Kind Withdrawal

An [in-kind distribution](https://www.investopedia.com/terms/i/in-kind.asp) returns the underlying assets, not cash. `withdraw` burns shares and pays out a proportional slice of the USDC vault and every asset vault. The user must already hold a token account for each asset; you can sell those on a DEX yourself.

---

## Program Flow

### Participants

- **Victor**, the registry authority: curates which assets, and which official Pyth feed, are safe to hold. A protocol role, not a manager.
- **Maria**, the strategy manager: earns a 1% annual fee running a basket she has a thesis on.
- **Alice**, the early depositor: wants diversified TSLAx and NVDAx exposure without managing positions.
- **Bob**, the later depositor: joins the same strategy after it has been running.

`Maria` and `Victor` are stored as plain `Pubkey`s and may each be a [Squads](https://squads.so/) multisig; the program only checks the signature.

### Victor creates the registry and whitelists assets

`initialize_registry()` creates a `Registry` PDA (`["registry", victor]`) owned by Victor. `whitelist_asset(price_feed)` then creates one `WhitelistEntry` PDA (`["whitelist", registry, mint]`) per approved mint, binding it to its official Pyth feed. Only Victor can do this. This separation is the anti-fraud core: a manager can only ever add assets Victor approved, and the feed comes from the registry, so a manager cannot list a token they mint themselves or pair a real mint with a feed they control.

### Maria initializes the strategy

`initialize_strategy(index=0, fee_bps=100, max_slippage_bps=100, swap_router)` creates the `Strategy` PDA (`["strategy", 0]`), the share mint, and the USDC vault, binding the strategy to Victor's registry. The strategy is addressed by a caller-chosen index (`"strategy" + 0`, `"strategy" + 1`, …) rather than the manager's key. No assets yet.

### Maria adds assets

`add_asset(weight_bps)`, once per asset, creates an `AssetConfig` at `["asset", strategy, index]` (index = current `asset_count`), copies the official feed from the whitelist entry, and creates that asset's vault. TSLAx at index 0 (4000 bps), NVDAx at index 1 (6000 bps). Rejected if the mint is not whitelisted, if the weights would exceed 10,000 bps, or once `MAX_ASSETS` (8) is reached. Deposits stay closed until the weights sum to exactly 10,000.

### Alice deposits, and her money is deployed at once

`deposit(usdc_amount, minimum_shares)`, with each asset's `[asset_config, vault, mint, rate, price_feed]` passed as remaining accounts, plus the router accounts. The handler requires the strategy to be fully allocated, values every asset for NAV (first deposit is 1:1), mints shares to Alice, then deploys her USDC across the basket at its target weights through the router, each leg under an oracle slippage floor. With the weights at 40/60, a 900 USDC deposit lands as 1.44 TSLAx and 3.0 NVDAx with no idle USDC.

### Bob deposits at the current share price

Same as Alice's deposit. Because shares are priced at NAV, Bob pays the current per-share value and does not dilute Alice's gain; his USDC is deployed at the target weights too.

### Maria rebalances

A price move pushes the basket off target. `rebalance(sell_amount, usdc_to_invest)` sells the over-weight asset for USDC and buys the under-weight one, both legs bounded against their Pyth prices, in one atomic instruction. `set_weight(weight_bps)` changes a target between rebalances, or retires an asset by setting it to zero (then reassign that weight to another asset to reach 100% again, and `rebalance` liquidates the retired holdings).

### Fees accrue

`collect_fees()` mints time-and-rate-proportional fee shares to Maria, diluting all holders by the fee.

### Alice withdraws in kind

`withdraw(shares_to_burn, min_usdc_out)`, with each asset's `[asset_config, vault, mint, user_token_account]` as remaining accounts. Alice's shares burn and she receives her proportional slice of USDC and every asset. Amounts floor in the protocol's favour.

---

## Oracle Integration (Pyth)

`PriceUpdateV2` price (i64) is read at byte offset 73 and `publish_time` at 93, directly from account bytes to avoid borsh version incompatibility with Anchor. Pyth USD pairs use exponent −8; with USDC and the basket tokens all at 6 decimals, value in USDC minor units is `amount × price / 10⁸`. Each asset's feed pubkey is fixed in its `AssetConfig` (copied from the registry), and validated on every read. In tests, mock `PriceUpdateV2` accounts are injected into LiteSVM (TSLAx $250, NVDAx $180).

---

## Mock Swap Router vs Production

The `mock-swap-router` exists only for testing: it stores a `usdc_per_token` rate per asset, holds the basket mints' authority, and mints/burns to simulate swaps. The `Strategy` stores the router program pubkey at creation, and `deposit` and `rebalance` require the router account to match it (`InvalidSwapRouter`). In production, replace the router CPIs with [Jupiter](https://jup.ag); the strategy PDA still signs.

---

## What restricts the manager

The strategy PDA holds all assets; no instruction moves a vault's tokens to the manager. The manager's powers are fenced:

- **Assets** are limited to mints whitelisted by the registry authority, with the price feed taken from the registry, not the manager.
- **Swaps** go only through the one router registered at creation, and each leg's minimum output is computed from the oracle, not supplied by the manager.
- **The fee** is fixed at creation and capped at 10%, paid only in minted shares.

What remains to trust: the honesty of the registered router and registry. With an honest router, the worst a careless manager can do is churn and pay market slippage (which hurts depositors but does not enrich the manager); the manager cannot withdraw principal.

---

## Financial Math Implementation

- Integer arithmetic only; intermediate products use `u128`; multiply before divide.
- All arithmetic uses `checked_*`. Users receive floor division; the protocol keeps the remainder.
- `transfer_checked` carries decimals through every token CPI.

---

## Build and Test

```bash
# Build each program on its own. Building the whole workspace at once unifies the
# vault's `cpi` feature into the router build and strips the router's entrypoint,
# leaving a stub .so, so build per-manifest (as `anchor build` does):
cargo build-sbf --manifest-path programs/mock-swap-router/Cargo.toml
cargo build-sbf --manifest-path programs/vault-strategy/Cargo.toml

# Run tests (LiteSVM, no local validator needed)
cargo test --manifest-path programs/vault-strategy/Cargo.toml
```

Tests live in `programs/vault-strategy/tests/vault_strategy.rs` and use [LiteSVM](https://github.com/LiteSVM/litesvm). Both `.so` files are loaded from `target/deploy/`, so build before testing. The suite covers the full lifecycle end to end (deposit with auto-deployment, a price move, rebalance back to target, a second depositor priced at the new NAV, a year's fee, in-kind withdrawal), retiring an asset with `set_weight` and reallocating to reopen deposits, and the rejection paths: non-whitelisted asset, weight overflow, over-cap fee and slippage, oracle-bounded deposit slippage, an under-allocated strategy, non-manager `set_weight`, unregistered router, and incomplete asset accounts on deposit.

## FAQ

### How do I build an onchain investment fund on Solana?

A manager creates a strategy with `initialize_strategy`, registers whitelisted assets with `add_asset` at target weights, and investors `deposit` USDC for shares. Each deposit is deployed across the basket in the same transaction, and `withdraw` redeems a proportional slice of every vault in kind.

### How are share prices calculated?

Shares are priced at the strategy's net asset value: the total value of the vault balances at current prices divided by shares outstanding. A later depositor pays the current share price rather than diluting earlier ones.

### How does the manager operate the fund?

`rebalance` trades the vaults back to their target weights as prices drift, `set_weight` reweights or retires an asset, and a management fee accrues over time and is collected with `collect_fees`.
