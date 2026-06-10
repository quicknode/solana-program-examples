# Vault Strategy

A manager-run investment vault on Solana. Users deposit [USDC](https://www.investopedia.com/terms/u/usd-coin-usdc.asp) and receive shares representing proportional ownership of a basket of assets. The manager allocates funds across the basket, earns a fee, and depositors withdraw their proportional slice when they choose.

The example uses two stocks as the basket assets: **TSLAx** (Tesla) and **NVDAx** (Nvidia) — [xStocks](https://backed.fi/xstocks) issued on Solana by Backed Finance. In tests these are mock [tokens](https://solana.com/docs/terminology#token).

---

## Programs

| Program | Description |
|---------|-------------|
| `vault-strategy` | Main vault: deposits, share minting, fee accrual, rebalancing, withdrawals |
| `mock-swap-router` | Test-only fake Jupiter. Stores exchange rates, mints/burns basket tokens for USDC. Replaced by real [Jupiter](https://jup.ag) in production. |

---

## Key Financial Concepts

### Net Asset Value (NAV)

[NAV](https://www.investopedia.com/terms/n/nav.asp) is the total dollar value of everything the vault holds right now. This vault computes it as:

```
NAV = vault_usdc_balance
    + vault_tsla_balance × tsla_price_in_usdc
    + vault_nvda_balance × nvda_price_in_usdc
```

NAV answers: *"if we liquidated the entire vault at today's prices, how many USDC would we get?"* It is used to price new deposits fairly — every depositor pays the same per-share price regardless of when they join.

Prices come from [Pyth Network](https://pyth.network/) oracle accounts (`PriceUpdateV2`). A staleness window of 60 seconds is enforced — deposits fail if either price is older than that.

### Shares

A [share](https://www.investopedia.com/terms/s/shares.asp) (also called an LP token or vault token) represents a fraction of the total vault. If you hold 1% of all shares, you own 1% of every asset in the vault.

- **First deposit**: shares are issued 1:1 with USDC base units (sets an initial share price of 1 USDC).
- **Later deposits**: `shares_to_mint = deposit_usdc × total_shares / NAV`. If the vault has grown, each new USDC buys fewer shares — correctly reflecting that the vault is worth more per share than when it started.
- Shares are [SPL tokens](https://solana.com/docs/terminology#token) stored in the depositor's [associated token account (ATA)](https://solana.com/docs/terminology#associated-token-account).

### Management Fee

A [management fee](https://www.investopedia.com/terms/m/managementfee.asp) is charged annually as a percentage of assets under management. This vault uses [basis points](https://www.investopedia.com/terms/b/basispoint.asp) (bps) — 100 bps = 1%.

The fee is collected by *minting new shares to the manager*, which dilutes existing holders proportionally. This avoids the need to know the current price at fee-collection time:

```
fee_shares = total_shares × fee_bps × elapsed_seconds / (10_000 × 31_536_000)
```

Anyone can call `collect_fees` — it is permissionless.

### Basket Allocation and Rebalancing

A [basket](https://www.investopedia.com/terms/b/basket.asp) is a group of assets held together. This vault targets a fixed allocation (e.g., 40% TSLAx, 60% NVDAx). Over time, price movements cause the actual allocation to drift from the target. [Rebalancing](https://www.investopedia.com/terms/r/rebalancing.asp) restores the target by selling the over-weight asset and buying the under-weight one.

### Slippage

[Slippage](https://www.investopedia.com/terms/s/slippage.asp) is the difference between the price you expected and the price you actually received. Every instruction that moves tokens accepts a `minimum_*` parameter — the transaction reverts if the output would fall below that floor.

### In-Kind Withdrawal

An [in-kind distribution](https://www.investopedia.com/terms/i/in-kind.asp) means you receive the underlying assets themselves, not cash. When you withdraw from this vault you receive a proportional slice of whatever the vault holds at that moment — some USDC, some TSLAx, some NVDAx — rather than a forced conversion to USDC. You can then sell those assets on a DEX yourself.

---

## Program Flow

### Participants

| Person | Role | Motivation |
|--------|------|-----------|
| **Alice** | Vault manager | Earn a 1% annual management fee on AUM; run a structured basket strategy she has a thesis on |
| **Bob** | Early depositor | Gain diversified exposure to TSLAx + NVDAx without managing individual positions |
| **Carol** | Later depositor | Join the same strategy after it has been running for a while |

Alice's `manager` key can be a [Squads](https://squads.so/) multisig address — the vault stores it as a plain `Pubkey` and checks only that the transaction is signed by it. No code change is needed to use a multisig.

---

### Step 1 — Alice initialises the vault

**Instruction:** `initialize_strategy(weight_bps_a=4000, weight_bps_b=6000, fee_bps=100, swap_router, price_feed_a, price_feed_b)`

The weights must sum to 10,000 bps, and `fee_bps` must not exceed `MAX_FEE_BPS` (1,000 bps = 10% per year). Because `collect_fees` mints shares to the manager and dilutes every depositor, an uncapped fee would let a manager drain the vault by configuration, so unsafe fees are rejected at creation time (`FeeTooHigh`).

**Accounts created:**

| Account | Seeds / Derivation | What it stores |
|---------|--------------------|----------------|
| `Strategy` [PDA](https://solana.com/docs/terminology#program-derived-address-pda) | `["strategy", alice_pubkey]` | manager, mint addresses, weights, fee, total shares, fee timestamp, swap router program pubkey, Pyth feed pubkeys |
| `share_mint` PDA | `["share_mint", strategy_pubkey]` | The SPL mint for vault shares. Strategy PDA is mint authority. |
| `vault_usdc` ATA | Associated token account of strategy PDA for USDC | Holds deposited USDC |
| `vault_asset_a` ATA | Associated token account of strategy PDA for TSLAx | Holds TSLAx after investing |
| `vault_asset_b` ATA | Associated token account of strategy PDA for NVDAx | Holds NVDAx after investing |

---

### Step 2 — Bob deposits 1,000 USDC

**Instruction:** `deposit(usdc_amount=1_000_000_000, minimum_shares=990_000_000)`

Pyth prices are read; NAV is computed. Since `total_shares == 0` this is the first deposit, so shares are issued 1:1.

**Accounts modified:**

| Account | Change |
|---------|--------|
| `bob_usdc_ata` | −1,000 USDC |
| `vault_usdc` | +1,000 USDC |
| `bob_share_ata` (created) | +1,000,000,000 shares |
| `strategy.total_shares` | 0 → 1,000,000,000 |

Bob now holds 100% of the vault. His motivation: rather than buying TSLAx and NVDAx directly and rebalancing himself, he trusts Alice's management and pays her 1% per year for the service.

---

### Step 3 — Alice invests: USDC → TSLAx and NVDAx

Alice calls `invest` twice, once per asset, to deploy the deposited USDC into the basket according to the 40/60 target.

**Instruction (call 1):** `invest(usdc_amount=400_000_000, minimum_asset_out=1_550_000)` — buys TSLAx at $250

**Accounts modified (call 1):**

| Account | Change |
|---------|--------|
| `vault_usdc` | −400 USDC |
| `vault_asset_a` (TSLAx) | +1,600,000 base units (1.6 TSLAx @ $250) |
| `router_usdc_treasury` | +400 USDC |

**Instruction (call 2):** `invest(usdc_amount=600_000_000, minimum_asset_out=3_300_000)` — buys NVDAx at $180

**Accounts modified (call 2):**

| Account | Change |
|---------|--------|
| `vault_usdc` | −600 USDC |
| `vault_asset_b` (NVDAx) | +3,333,333 base units (3.33 NVDAx @ $180) |
| `router_usdc_treasury` | +600 USDC |

After both calls the vault holds: ~0 USDC, 1.6 TSLAx, 3.33 NVDAx — all worth ~1,000 USDC at current prices.

---

### Step 4 — Carol deposits 1,000 USDC (after investing)

**Instruction:** `deposit(usdc_amount=1_000_000_000, minimum_shares=990_000_000)`

Pyth prices are read. NAV ≈ 1,000 USDC (same total value as before, just now held as basket tokens). The share price is still ~1 USDC per share, so Carol receives approximately the same number of shares as Bob.

`shares_to_mint = 1,000 USDC × 1,000,000,000 shares / 1,000 USDC NAV ≈ 1,000,000,000`

**Accounts modified:**

| Account | Change |
|---------|--------|
| `carol_usdc_ata` | −1,000 USDC |
| `vault_usdc` | +1,000 USDC |
| `carol_share_ata` (created) | +~1,000,000,000 shares |
| `strategy.total_shares` | ~1,000,000,000 → ~2,000,000,000 |

Bob and Carol now each own ~50% of the vault.

---

### Step 5 — Alice rebalances (optional)

Suppose TSLAx has risen and the allocation has drifted to 45% TSLAx / 55% NVDAx. Alice calls `rebalance` to sell some TSLAx and buy more NVDAx, restoring the 40/60 target.

**Instruction:** `rebalance(sell_amount=800_000, minimum_usdc_from_sell=195_000_000, usdc_to_invest=200_000_000, minimum_buy_amount=1_100_000)`

Two CPI legs execute atomically:
1. Sell 800,000 TSLAx base units → receive ~200 USDC from router treasury
2. Buy NVDAx with 200 USDC → receive ~1,111,111 NVDAx base units

**Accounts modified:**

| Account | Change |
|---------|--------|
| `vault_asset_a` (TSLAx) | −800,000 base units |
| `vault_usdc` | net zero (briefly +200 USDC, then −200 USDC) |
| `vault_asset_b` (NVDAx) | +1,111,111 base units |
| `router_usdc_treasury` | net: +USDC from TSLAx sale, −USDC for NVDAx purchase |

If either slippage check fails, both legs revert — no partial rebalance.

---

### Step 6 — Alice collects fees

Six months have elapsed. Anyone calls `collect_fees` (it is permissionless).

**Instruction:** `collect_fees()`

```
fee_shares = 2,000,000,000 × 100 bps × 15,768,000 s / (10,000 × 31,536,000 s) ≈ 10,000,000
```

**Accounts modified:**

| Account | Change |
|---------|--------|
| `alice_share_ata` (created if needed) | +10,000,000 shares |
| `share_mint` total supply | +10,000,000 |
| `strategy.total_shares` | → ~2,010,000,000 |
| `strategy.last_fee_accrual_timestamp` | updated to now |

Bob and Carol are each diluted by ~0.5%. Alice now holds ~0.5% of the vault.

---

### Step 7 — Bob withdraws

Bob burns all his shares and receives his proportional slice of the vault in-kind.

**Instruction:** `withdraw(shares_to_burn=1_000_000_000, min_usdc_out=0, min_asset_a_out=0, min_asset_b_out=0)`

Bob's proportion: 1,000,000,000 / 2,010,000,000 ≈ 49.75%

**Accounts modified:**

| Account | Change |
|---------|--------|
| `bob_share_ata` | −1,000,000,000 (burned) |
| `share_mint` total supply | −1,000,000,000 |
| `strategy.total_shares` | −1,000,000,000 |
| `vault_usdc` | −~497 USDC |
| `vault_asset_a` (TSLAx) | −~49.75% of TSLAx balance |
| `vault_asset_b` (NVDAx) | −~49.75% of NVDAx balance |
| `bob_usdc_ata` | +~497 USDC |
| `bob_tsla_ata` (created if needed) | +proportional TSLAx |
| `bob_nvda_ata` (created if needed) | +proportional NVDAx |

Bob receives TSLAx and NVDAx directly in his own ATAs. He can sell them on a DEX if he wants USDC back.

---

## Instruction Reference

| Instruction | Signer | Key Accounts Read | Key Accounts Written |
|------------|--------|-------------------|----------------------|
| `initialize_strategy` | manager | — | Strategy PDA, share_mint, vault_usdc, vault_asset_a, vault_asset_b |
| `deposit` | depositor | vault_usdc, vault_asset_a, vault_asset_b, price_feed_a, price_feed_b | vault_usdc (+), depositor_usdc_ata (−), depositor_share_ata (+), strategy.total_shares (+) |
| `invest` | manager | strategy | vault_usdc (−), vault_asset (+), router_usdc_treasury (+) |
| `rebalance` | manager | strategy | vault_sell (−), vault_buy (+), vault_usdc (net 0), router_usdc_treasury |
| `collect_fees` | payer (anyone) | strategy, clock | manager_share_ata (+), share_mint supply (+), strategy.total_shares (+), strategy.last_fee_accrual_timestamp |
| `withdraw` | user | strategy | user_share_ata (−), vault_usdc (−), vault_asset_a (−), vault_asset_b (−), user_usdc_ata (+), user_asset_a_ata (+), user_asset_b_ata (+), strategy.total_shares (−) |

---

## Oracle Integration (Pyth)

Prices come from [Pyth Network](https://pyth.network/) `PriceUpdateV2` accounts. Two feed pubkeys are stored in the `Strategy` account at creation time and validated on every deposit via a key constraint.

- Pyth USD pairs report price with exponent −8 (i.e., `price × 10⁻⁸ = USD per token`)
- With both USDC and basket tokens using 6 decimal places, the scaling cancels: `usdc_base_per_token_base = price / 10⁸`
- Prices older than 60 seconds are rejected (`StalePriceFeed`)
- Zero or negative prices are rejected (`NegativePrice`)
- Price data is read from raw account bytes at fixed offsets to avoid borsh version incompatibility between the Pyth SDK and Anchor 1.0

In tests, mock `PriceUpdateV2` accounts are injected directly into LiteSVM with the Pyth Receiver program as owner (TSLAx at $250, NVDAx at $180).

---

## Mock Swap Router vs Production

The `mock-swap-router` exists only for testing. It:
- Stores `usdc_per_token` rate in an `AssetRate` PDA per basket token
- Acts as mint authority for basket tokens (`router_authority` PDA signs mint CPIs)
- `swap_usdc_for_asset`: receives USDC into its treasury, mints basket tokens to caller
- `swap_asset_for_usdc`: burns basket tokens from caller, releases USDC from its treasury

The `Strategy` account stores the router's program pubkey (`swap_router`) at creation time, and `invest` and `rebalance` require the swap router program account they are given to match it (`InvalidSwapRouter`). A manager cannot route vault funds through a program the strategy did not register.

In production, replace the router CPIs in `invest` and `rebalance` with [Jupiter](https://jup.ag) CPI calls. The strategy PDA still signs; only the target program ID and account list change.

---

## Account Validation

Every account a caller passes is checked against state the program controls, never trusted:

- **Mints are bound to the strategy.** `deposit` and `withdraw` enforce `has_one` on `usdc_mint`, `asset_mint_a`, and `asset_mint_b` against the pubkeys stored in the `Strategy` account (`InvalidUsdcMint` / `InvalidAssetMint`). Without this, a caller could pass an unregistered mint whose strategy-owned vault is empty, understating NAV to mint inflated shares on deposit or skewing the proportional payout on withdraw. `invest` and `rebalance` enforce `has_one` on `usdc_mint` and require their asset mints to be one of the two registered basket mints.
- **Vault token accounts are derived, not supplied.** Each vault account must be the associated token account of the strategy PDA for the corresponding bound mint.
- **Price feeds are bound to the strategy.** The Pyth accounts passed to `deposit` must equal the feed pubkeys stored at creation (`InvalidPriceFeed`).
- **The swap router is bound to the strategy.** `invest` and `rebalance` require the router program account to equal the stored `swap_router` (`InvalidSwapRouter`).
- **Config is validated at creation.** Weights must sum to 10,000 bps and the fee is capped at `MAX_FEE_BPS`.

---

## Custody and Trust

This is a **manager-custodial** vault. The strategy [PDA](https://solana.com/docs/terminology#program-derived-address-pda) holds all assets; the manager controls `invest` and `rebalance` with no onchain constraint that they follow the stated allocation. Depositors trust the manager to act in their interest.

The `manager` field is a plain `Pubkey`. It can be a [Squads](https://squads.so/) multisig address — the vault checks only that the transaction carries a valid signature from that key. Squads handles threshold approval before the transaction reaches the vault. No program changes are required.

---

## Financial Math Implementation

- No floating point — integer arithmetic only throughout
- All intermediate products use `u128` to prevent overflow (`u64 × u64` overflows at ~1.8 × 10¹⁹)
- Multiply before divide to preserve precision
- All arithmetic uses `checked_*` methods — raw `+ - * /` are never used on token amounts
- The user always receives floor division; the protocol retains the rounding remainder
- `transfer_checked` is used for all SPL token transfers (carries decimals through the CPI to catch wrong-mint errors)

---

## Build and Test

```bash
# Build the vault (requires the Solana toolchain). This also compiles the
# router, but with the vault's `cpi` feature enabled, which strips the
# router's entrypoint and leaves a stub .so:
cargo build-sbf

# So build the router again on its own to get a deployable .so:
cargo build-sbf --manifest-path programs/mock-swap-router/Cargo.toml

# Run tests (LiteSVM, no local validator needed)
cargo test
```

Tests live in `programs/vault-strategy/tests/vault_strategy.rs` and use [LiteSVM](https://github.com/LiteSVM/litesvm) for fast, self-contained program simulation. Both `.so` files are loaded from `target/deploy/`, so build before testing. The suite exercises all six instruction handlers and the rejection paths: slippage limits, unregistered mints on deposit and withdraw, an over-cap management fee, and an unregistered swap router on invest and rebalance.
