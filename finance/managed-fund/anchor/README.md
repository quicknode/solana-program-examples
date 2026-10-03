# Solana Managed Fund (Anchor)

> [!NOTE]
> This is the **Anchor v2** copy of this example. Every `anchor` command on this page
> needs the v2 CLI: `cargo install anchor-cli --version 2.0.0-rc.1 --locked` (avm has
> no prebuilt binary for this pre-release). The Anchor v1 version of this example is in
> [`../anchor-v1`](../anchor-v1/).

A manager-run investment fund on Solana, the onchain equivalent of a mutual fund. Users deposit [USDC](https://www.investopedia.com/terms/u/usd-coin-usdc.asp) and receive shares representing proportional ownership of a portfolio of assets. The manager adds assets a curator has approved and sets their target weights; each deposit is deployed across those assets at its weights in the same transaction. The manager rebalances as prices drift, earns a fee, and depositors withdraw their proportional slice in kind when they choose.

The example uses two stocks as the portfolio assets: **TSLAx** (Tesla) and **NVDAx** (NVIDIA) - [xStocks](https://backed.fi/xstocks) issued on Solana by Backed Finance. In tests these are mock [tokens](https://solana.com/docs/terminology#token).

A note on the word **vault**: by the common standard (ERC-4626) a vault holds a single asset. Here a vault is one single-asset [token account](https://solana.com/docs/terminology#token-account), and the whole multi-asset construct is the **fund**, which owns one vault per asset plus a USDC vault. Some platforms call the whole product a vault, or a **vault strategy**. This example was called `vault-strategy` until it was renamed `managed-fund`, so a search for either name lands here.

---

## Programs

- **`managed-fund`**: Registry and approved assets, fund creation, asset registration, deposits, share minting, fee accrual, rebalancing, withdrawals
- **`mock-swap-router`**: Test-only fake Jupiter. Stores exchange rates, mints/burns basket tokens for USDC. Replaced by real [Jupiter](https://jup.ag) in production.

---

## Key Financial Concepts

### Net Asset Value (NAV)

[NAV](https://www.investopedia.com/terms/n/nav.asp) is the total value of everything the fund holds: its USDC plus each asset valued at its Pyth price. It prices new deposits fairly, so every depositor pays the same per-share price regardless of when they join.

The amounts come from the fund's own records, `usdc_holdings` and `asset_holdings`, not from the vaults' token balances. Deposits, swaps and withdrawals update them with what each transfer actually moved, so they always equal what the fund owns. Anyone can transfer tokens straight into a vault, and those tokens (a donation) are outside the fund: they change a vault's balance and nothing the program reads. That is the defense against the first-depositor inflation attack, where a dust-sized first deposit followed by a donation would otherwise price one share above the next deposit and round it down to zero shares. Donated tokens are never paid out, and `rebalance` can neither sell nor spend them (`InsufficientHoldings`).

Because the asset set is dynamic, `deposit` must value *every* asset. The assets live at PDAs indexed `0..asset_count`, and `deposit` re-derives that complete range from the accounts it is given, refusing to run if any asset is missing (`IncompleteAssetAccounts`). This makes it structurally impossible to omit an asset and understate NAV.

Referencing every asset has a transaction-size cost: `deposit` pulls in `14 + 5N` accounts and `withdraw` `10 + 4N`, where `N` is the asset count. That stays within Solana's 128-account transaction lock limit at the `MAX_ASSETS` cap of 16 (94 accounts for `deposit`), but a basket beyond roughly three assets no longer fits a legacy transaction's 1232-byte limit, so the client must send a v0 transaction with an [Address Lookup Table](https://docs.anza.xyz/proposals/versioned-transactions).

Prices come from [Pyth Network](https://pyth.network/) `PriceUpdateV2` accounts. A 60-second staleness window is enforced; zero or negative prices are rejected, and so is any price posted at or before the last cluster restart (`PricePredatesRestart`), which the seconds check alone cannot catch after a halt. A price whose confidence interval is wider than 1% of the price (`MAX_CONFIDENCE_BPS`, 100) is rejected too (`OracleConfidenceTooWide`): deposits price shares from the oracle and rebalance sets its swap floor from it, so a price the publishers disagree on by more than a typical slippage tolerance is not one to trade on. `withdraw` reads no price, so investors can always leave in kind while deposits and rebalances wait for the band to narrow.

### Shares

A [share](https://www.investopedia.com/terms/s/shares.asp) represents a fraction of the whole fund. Hold 1% of shares and you own 1% of every vault.

- **First deposit**: shares are issued 1:1 with USDC minor units (initial price of 1 USDC per share).
- **Later deposits**: `shares_to_mint = deposit_usdc × total_shares / NAV`, with NAV valued from the recorded holdings.
- **A deposit leg must buy something.** A deposit so small that one of its swaps spends USDC and returns none of the asset is refused (`DepositTooSmall`). Otherwise it would mint shares against no recorded value, and every later deposit would divide by a zero NAV.
- Shares are [SPL tokens](https://solana.com/docs/terminology#token); the share mint's address is a [PDA](https://solana.com/docs/terminology#program-derived-address-pda), so it is deterministic and the fund PDA is its mint authority.

### Management Fee

A [management fee](https://www.investopedia.com/terms/m/managementfee.asp), in [basis points](https://www.investopedia.com/terms/b/basispoint.asp) (100 bps = 1% per year), is charged by *minting new shares to the manager*, diluting holders proportionally. This is the common onchain pattern (Yearn, Lido charge fees this way) and differs from a traditional fund, which deducts the fee in cash from assets.

```
fee_shares = total_shares × fee_bps × elapsed_seconds / (10_000 × 31_536_000)
```

`collect_fees` is permissionless. The fee is fixed at creation and capped at `MAX_FEE_BPS` (1,000 bps = 10%); there is no setter to raise it later.

### Weights and Rebalancing

Each asset carries a target **weight** in basis points (e.g. 40% TSLAx, 60% NVDAx). A fund accepts deposits only once its weights sum to exactly 10,000 (`add_asset` and `set_weight` keep the running sum at or below 10,000; `deposit` requires it to equal 10,000, else `FundNotFullyAllocated`). So a fund is either still being configured or fully allocated and live, and `deposit` deploys each depositor's USDC straight into the basket at those weights, fully invested bar sub-cent rounding dust. There is no idle-cash mode.

[Rebalancing](https://www.investopedia.com/terms/r/rebalancing.asp) handles the drift that prices create after a deposit: `rebalance(sell_index, buy_index)` sells an over-weight asset for USDC and buys an under-weight one in a single atomic instruction. **Anyone may call it, and the program, not the caller, sizes the trade.** It values every asset (the same five accounts per asset as `deposit`), and requires the asset sold to sit above its target by at least the fund's `rebalance_threshold_bps` of the fund's value (`DriftBelowThreshold`) and the asset bought to sit below its own (`NotUnderweight`). It then trades the smaller of the two gaps, so neither asset ends past its target, and spends only what the sale brought in. A fund at its targets has no trade to make, so nobody, the manager included, can churn it: trade it back and forth to bleed value through slippage. The threshold is set at creation between `MIN_REBALANCE_THRESHOLD_BPS` (100, one percentage point) and `MAX_REBALANCE_THRESHOLD_BPS` (2,000) and has no setter (`RebalanceThresholdOutOfRange`). `set_weight` changes a target after creation, including setting it to zero to **retire** an asset: deposits stop allocating to it, a rebalance may sell all of it whatever the threshold, and the now-empty vault keeps its index so the contiguous `0..asset_count` range stays intact (the index is never reused).

### Slippage, bounded by the oracle

[Slippage](https://www.investopedia.com/terms/s/slippage.asp) is the gap between the expected and the realized amount of a swap. Rather than trust a manager-supplied minimum, `deposit` and `rebalance` compute the floor themselves from the Pyth price and the fund's `max_slippage_bps`: a swap whose output falls more than that tolerance below the oracle-implied amount reverts. `max_slippage_bps` is set at creation and capped at `MAX_SLIPPAGE_BPS` (1,000 bps = 10%).

### In-Kind Withdrawal

An [in-kind distribution](https://www.investopedia.com/terms/i/in-kind.asp) returns the underlying assets, not cash. `withdraw` burns shares and pays out a proportional slice of the USDC vault and every asset vault. The user must already hold a token account for each asset; you can sell those on a DEX yourself.

---

## Program Flow

### Participants

- **Victor**, the registry authority: curates which assets, and which official Pyth feed, are safe to hold. A role in the program, separate from the managers.
- **Maria**, the fund manager: earns a 1% annual fee running a basket she has a thesis on.
- **Alice**, the early depositor: wants diversified TSLAx and NVDAx exposure without managing positions.
- **Bob**, the later depositor: joins the same fund after it has been running.

`Maria` and `Victor` are stored as plain `Pubkey`s and may each be a [Squads](https://squads.so/) multisig; the program only checks the signature.

### Victor creates the registry and approves assets

`initialize_registry()` creates a `Registry` PDA (`["registry", victor]`) owned by Victor. The registry holds no list; it only names the curator. The approved set is the collection of `ApprovedAsset` accounts under it: `approve_asset(price_feed)` creates one `ApprovedAsset` PDA (`["approved_asset", registry, mint]`) per approved mint, binding it to its official Pyth feed, and an asset counts as approved exactly when that account exists. Only Victor can create them. This separation is the anti-fraud core: a manager can only ever add assets Victor approved, and the feed comes from the registry, so a manager cannot list a token they mint themselves or pair a real mint with a feed they control.

### Maria initializes the fund

`initialize_fund(index=0, fee_bps=100, max_slippage_bps=100, swap_router)` creates the `Fund` PDA (`["fund", 0]`), the share mint, and the USDC vault, binding the fund to Victor's registry. The fund is addressed by a caller-chosen index (`"fund" + 0`, `"fund" + 1`, …) rather than the manager's key. No assets yet.

### Maria adds assets

`add_asset(weight_bps)`, once per asset, creates an `AssetConfig` at `["asset", fund, index]` (index = current `asset_count`), copies the official feed from the ApprovedAsset account, and creates that asset's vault. TSLAx at index 0 (4000 bps), NVDAx at index 1 (6000 bps). Rejected if the mint is not approved (its ApprovedAsset account does not exist), if the weights would exceed 10,000 bps, or once `MAX_ASSETS` (16) is reached. Deposits stay closed until the weights sum to exactly 10,000.

### Alice deposits, and her money is deployed at once

`deposit(usdc_amount, minimum_shares)`, with each asset's `[asset_config, vault, mint, rate, price_feed]` passed as remaining accounts, plus the router accounts. The handler requires the fund to be fully allocated, values every asset's recorded holding for NAV (first deposit is 1:1), mints shares to Alice, then deploys her USDC across the basket at its target weights through the router, each leg under an oracle slippage floor. With the weights at 40/60, a 900 USDC deposit lands as 1.44 TSLAx and 3.0 NVDAx with no idle USDC.

### Bob deposits at the current share price

Same as Alice's deposit. Because shares are priced at NAV, Bob pays the current per-share value and does not dilute Alice's gain; his USDC is deployed at the target weights too.

### Anyone rebalances

A price move pushes the basket off target by more than the fund's threshold. Anyone calls `rebalance(sell_index, buy_index)`, naming the over-weight asset and an under-weight one; the program computes how much to sell from the Pyth prices and the target weights, and both legs are bounded against their Pyth prices, in one atomic instruction. `set_weight(weight_bps)` is how Maria steers the fund: it changes a target, or retires an asset by setting it to zero (then reassign that weight to another asset to reach 100% again, and `rebalance` sells the retired holdings).

### Fees accrue

`collect_fees()` mints time-and-rate-proportional fee shares to Maria, diluting all holders by the fee.

### Alice withdraws in kind

`withdraw(shares_to_burn, min_usdc_out)`, with each asset's `[asset_config, vault, mint, user_token_account]` as remaining accounts. Alice's shares burn and she receives her proportional slice of USDC and every asset. Amounts floor in the program's favour.

---

## Oracle Integration (Pyth)

`PriceUpdateV2` price (i64) is read at byte offset 73, `publish_time` at 93 and `posted_slot` (u64) at 125, directly from account bytes to avoid borsh version incompatibility with Anchor. Under Alpenglow each leader sets the Clock's `unix_timestamp`, which may advance by at most twice the slot time elapsed since the parent block, so after a halt the timestamp trails real time and a price published just before the halt can still look fresh by seconds. `load_price` therefore also requires `posted_slot` to be after the `LastRestartSlot` sysvar's slot (0 means the cluster has never restarted), pausing deposits and rebalances until Pyth posts again. The exponent (i32) is read at offset 89 rather than assumed: Pyth's crypto USD feeds use −8 and its US equity feeds −5. Each asset's mint decimals are recorded in its `AssetConfig` when it is added, and the USDC mint's in the `Fund` at creation, so value in USDC minor units is `amount × price × 10^(usdc_decimals + exponent − asset_decimals)`: `amount × price / 10⁸` only when the asset has USDC's 6 decimals and the feed's exponent is −8. Each asset's feed pubkey is fixed in its `AssetConfig` (copied from the registry), and validated on every read. In tests, mock `PriceUpdateV2` accounts are injected into LiteSVM (TSLAx $250, NVDAx $180).

---

## Mock Swap Router vs Production

The `mock-swap-router` exists only for testing: it stores a `usdc_per_token` rate per asset (USDC minor units per whole token, scaled by the asset mint's decimals on each swap), holds the basket mints' authority, and mints/burns to simulate swaps. Its single `router_config` account (`["router_config"]`) is both its state and its signer: it owns the USDC treasury, is the mint authority of every basket mint, and signs the router's token CPIs with its own seeds, the same way the fund PDA does for the vaults. The `Fund` stores the router program pubkey at creation, and `deposit` and `rebalance` require the router account to match it (`InvalidSwapRouter`). In production, replace the router CPIs with [Jupiter](https://jup.ag); the fund PDA still signs.

---

## What restricts the manager

The fund PDA holds all assets; no instruction moves a vault's tokens to the manager. The manager's powers are fenced:

- **Assets** are limited to mints approved by the registry authority, with the price feed taken from the registry, not the manager.
- **Swaps** go only through the one router registered at creation, and each leg's minimum output is computed from the oracle, not supplied by the manager.
- **Rebalancing** is sized by the program from the oracle and the target weights, and only once drift passes a threshold fixed at creation. The manager chooses the weights, not the trades.
- **The fee** is fixed at creation and capped at 10%, paid only in minted shares.

What remains to trust: the honesty of the registered router and registry. The manager cannot withdraw principal, and cannot churn the fund through the router either: no caller chooses a trade's size or can trade a fund sitting at its targets.

---

## Financial Math Implementation

- Integer arithmetic only; intermediate products use `u128`; multiply before divide.
- All arithmetic uses `checked_*`. Users receive floor division; the program keeps the remainder.
- `transfer_checked` carries decimals through every token CPI.

---

## Build and Test

```bash
# Build each program on its own. Building the whole workspace at once unifies the
# fund's `cpi` feature into the router build and strips the router's entrypoint,
# leaving a stub .so, so build per-manifest (as `anchor build` does):
cargo build-sbf --manifest-path programs/mock-swap-router/Cargo.toml
cargo build-sbf --manifest-path programs/managed-fund/Cargo.toml

# Run tests (LiteSVM, no local validator needed)
cargo test --manifest-path programs/managed-fund/Cargo.toml
```

Tests live in `programs/managed-fund/tests/managed_fund.rs` and use [LiteSVM](https://github.com/LiteSVM/litesvm). Both `.so` files are loaded from `target/deploy/`, so build before testing. The suite covers the full lifecycle end to end (deposit with auto-deployment, a price move, rebalance back to target, a second depositor priced at the new NAV, a year's fee, in-kind withdrawal), retiring an asset with `set_weight` and reallocating to reopen deposits, and the rejection paths: unapproved asset, weight overflow, over-cap fee and slippage, oracle-bounded deposit slippage, an under-allocated fund, non-manager `set_weight`, unregistered router, and incomplete asset accounts on deposit and rebalance. The rebalance tests sign as a stranger, since anyone may call it: `test_rebalance_refuses_fund_at_target`, `test_rebalance_refuses_drift_below_threshold` and `test_rebalance_cannot_churn` check that a fund at its targets, or within its threshold, or just rebalanced, cannot be traded; `test_rebalance_refuses_buying_overweight_asset`, `test_rebalance_sells_retired_asset` and `test_initialize_rejects_threshold_out_of_range` cover the rest of its rules. `test_valuation_scales_by_decimals_and_exponent` runs the story with an eight-decimal TSLAx priced by an exponent −5 feed and gets the same share counts. `test_wide_confidence_price_rejected` widens NVDAx's confidence interval to 2% of its price and checks that deposit and rebalance fail with `OracleConfidenceTooWide`, that withdraw still pays out in kind, and that a band of exactly 1% is accepted. `test_full_lifecycle` checks after every step that the recorded holdings equal the vaults' balances. `test_donation_does_not_inflate_share_price` runs the first-depositor attack (a one-minor-unit deposit, a 1,000 USDC transfer straight into the USDC vault, then a 1,000 USDC deposit with no `minimum_shares` floor) and checks the victim gets exactly the shares they would have got without the donation. `test_deposit_rejects_leg_that_buys_nothing` and `test_rebalance_ignores_donations` pin the other two guards: the second checks that donated tokens can neither force a rebalance nor be spent by one.

## FAQ

### How do I build an onchain investment fund on Solana?

A manager creates a fund with `initialize_fund`, registers curator-approved assets with `add_asset` at target weights, and investors `deposit` USDC for shares. Each deposit is deployed across the basket in the same transaction, and `withdraw` redeems a proportional slice of every vault in kind.

### How are share prices calculated?

Shares are priced at the fund's net asset value: the total value of its recorded holdings at current prices divided by shares outstanding. A later depositor pays the current share price rather than diluting earlier ones. Tokens transferred straight into a vault are not part of the recorded holdings, so they cannot move the share price.

### How does the manager operate the fund?

`set_weight` reweights or retires an asset, `rebalance`, which anyone may call, trades the vaults back to those weights once prices drift past the fund's threshold, and a management fee accrues over time and is collected with `collect_fees`.
