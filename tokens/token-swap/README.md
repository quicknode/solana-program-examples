# Token Swap (AMM)

A Constant Product Automated Market Maker (AMM) in [Anchor](https://solana.com/docs/terminology#anchor) — the model popularized by Uniswap V2.

The pool keeps `x * y = K` invariant: if `x` is the reserve of token A and `y` is the reserve of token B, then `x * y` stays constant for a given liquidity quantity.

## What this example includes

- A singleton `Config` [PDA](https://solana.com/docs/terminology#program-derived-address-pda) at seeds `[b"config"]` holding the trading-fee bps, admin-share bps, and admin authority.
- A unique pool PDA per `(config, mint_a, mint_b)`, with `mint_a < mint_b` for canonical addressing.
- LP positions tracked as SPL tokens via a per-pool `liquidity_provider_mint`, so they're composable with any wallet or downstream [program](https://solana.com/docs/terminology#program).
- Deposits clamped to the current pool ratio (Uniswap V2's `mint()` pattern), with caller amounts treated as upper bounds and all ratio math done in `u128` with checked arithmetic.
- Constant-product (`x * y = k`) swaps with a trading fee split between LPs and the admin, configured by `Config.fee` and `Config.admin_share_bps`.
- Admin protocol fees accrued onchain as virtual claims on the pool reserves, swept on demand by `claim_admin_fees`.
- Withdrawals proportional to LP-token share of the **effective reserves** (raw reserve minus admin's owed slice), so the admin's accrued fees don't dilute exiting LPs.
- **Caller-supplied slippage floors on every state-changing instruction:** swaps revert with `SlippageExceeded` if the output falls below `min_output_amount`, deposits revert with `DepositBelowMinimum` if the LP mint amount falls below `minimum_lp_tokens_out`, withdrawals revert with `WithdrawalBelowMinimum` if either side falls below its floor.
- **Defence-in-depth invariant check:** every swap re-verifies `effective_pool_a * effective_pool_b` doesn't decrease after the transfers, so a bug in the curve math fails the transaction instead of silently giving the trader too much.
- All financial math in `u128` with checked arithmetic, matching how production Solana AMMs (Orca, Raydium, Meteora, Saber) do it.
- Anchor 1.0 Rust [program](https://solana.com/docs/terminology#program) with LiteSVM integration tests.

## Why a CPAMM

Other bonding-curve designs exist:

- **Constant Sum AMM (CSAMM):** `x + y = K`. Constant price but reserves can be drained.
- **Curve Stableswap:** a mix of CSAMM and CPAMM, tuned for like-priced assets.
- **Uniswap V3 Concentrated Liquidity AMM (CLAMM):** splits the curve into buckets; LPs supply liquidity to specific price ranges.
- **Trader Joe CLAMM:** like Uniswap V3, but each bucket is a CSAMM.

A CPAMM is the simplest and the cheapest to keep in [account](https://solana.com/docs/terminology#account) state — one pool, one [mint](https://solana.com/docs/terminology#token-mint), easy to reason about. That's what this example implements.

## Design

Requirements:

- **Fee distribution.** Every pool charges a trading fee, paid in the traded token, that rewards LPs. To stay consistent across pools, the fee is shared.
- **Single pool per asset pair.** Avoids liquidity fragmentation.
- **LP accounting.** The [program](https://solana.com/docs/terminology#program) tracks each LP's deposits.

Implementation choices:

- **Singleton config.** A single `Config` account stores the shared trading-fee config and admin. It is a global singleton: one per deployed program, derived at the fixed seed `[b"config"]`. This matches how real DEXs are deployed in practice (Phoenix, Raydium, etc. ship one program per market/AMM), and keeps the example simpler than parameterising the config by an id.
- **Unique pools.** Each pool is a [PDA](https://solana.com/docs/terminology#program-derived-address-pda) seeded from the `Config`, `mint_a`, and `mint_b` (in that order, with `mint_a < mint_b`).
- **LP accounting via tokens.** LP positions are tracked as tokens (the `liquidity_provider_mint`), so they're composable with any wallet or downstream program.

## Onchain-design principles applied here

- **Store keys in the account.** Even for PDAs, storing the parent keys in the account state makes lookups easier (you can rebuild the PDA without consulting external data) and works well with Anchor's `has_one` constraint.
- **Keep seeds simple.** Start with the parent's seeds, then the current object's identifiers in alphabetical order. For the pool, that means `[config, mint_a, mint_b]`.
- **Keep [instruction](https://solana.com/docs/terminology#instruction) scope small.** Smaller instructions touch fewer accounts, leaving room in the transaction and improving composability and security.

## File structure

```text
programs/token-swap/src/
├── constants.rs
├── errors.rs
├── instructions
│   ├── claim_admin_fees.rs
│   ├── create_config.rs
│   ├── create_pool.rs
│   ├── deposit_liquidity.rs
│   ├── mod.rs
│   ├── swap_tokens.rs
│   └── withdraw_liquidity.rs
├── lib.rs
└── state/
    ├── config.rs
    ├── mod.rs
    └── pool_config.rs
```

## State

### `Config`

Shared configuration for the AMM. **Singleton** — one per deployed program, at PDA seeds `[b"config"]`.

- `admin: Pubkey` — the admin authority. Only this address can call `claim_admin_fees`.
- `fee: u16` — total trading fee in basis points (must be < 10000). Split between LPs and the admin according to `admin_share_bps`.
- `admin_share_bps: u16` — fraction of the trading fee that goes to the admin, in basis points (must be < 10000). The remainder goes to LPs. Modelled on Uniswap V2 / Raydium: the AMM operator takes a slice of every fee, LPs keep the rest.

### `PoolConfig`

Per-pool configuration / identity record. Identifies a single pool by which `Config` it belongs to and which two mints it trades, and tracks the admin's accumulated trading-fee claim for each side. The actual pool reserves live in separate token accounts (`pool_a`, `pool_b`) owned by `pool_authority` — they are *not* stored here.

- `config: Pubkey` — the parent `Config` account.
- `mint_a: Pubkey` — mint of token A.
- `mint_b: Pubkey` — mint of token B.
- `admin_fees_owed_a: u64` — admin's accumulated fee claim on token A, in base units. Sits physically in `pool_a` but is excluded from the LP curve and from LP-withdrawable amounts. Swept by `claim_admin_fees`.
- `admin_fees_owed_b: u64` — same for token B.

The admin's fees are tracked as *virtual* claims on the existing `pool_a` / `pool_b` reserves rather than as separate vaults. LP-facing math uses **effective reserves** = `pool_X.amount - admin_fees_owed_X` so the admin's owed slice doesn't grow LP yield.

`PoolConfig` PDA seeds: `[config, mint_a, mint_b]` with `mint_a < mint_b`.

## Instruction handlers

### `create_config`

Initializes the singleton `Config` account with the supplied `admin`, `fee`, and `admin_share_bps`. The `Config` PDA is derived from the fixed seed `[b"config"]`, so this can only succeed once per deployed program. Enforces `fee < 10000` and `admin_share_bps < 10000`.

### `create_pool`

Initializes a `PoolConfig` account, an LP mint (`liquidity_provider_mint`), and the two pool reserve token accounts (`pool_a`, `pool_b`) owned by `pool_authority`. Enforces `mint_a < mint_b` for canonical pool addressing.

### `deposit_liquidity`

Transfers token A and token B from the depositor to the pool, then mints LP tokens to the depositor. `amount_a` and `amount_b` are treated as **upper bounds** — the caller's maximum willingness on each side. The contract clamps both numbers down to the largest pair that lies on the current price line, then pulls exactly that pair. `minimum_lp_tokens_out` is the caller's **lower bound** on what they're willing to receive in LP tokens; the handler reverts with `DepositBelowMinimum` if the post-clamp LP mint amount falls below it. Pass `0` to opt out (any non-zero mint is acceptable).

- For the first deposit, both amounts are used as-is and the LP amount is `sqrt(amount_a * amount_b)` — computed with a `u128` integer-sqrt (Newton's method), no floats — with `MINIMUM_LIQUIDITY` locked away forever (to prevent the empty-pool edge case). No admin fees can be owed yet, so this case is unchanged by the admin-fee mechanism.
- For later deposits, the amounts are clamped to the current pool ratio (Uniswap V2's `mint()` pattern):
  1. Compute `amount_b_required = amount_a * effective_pool_b / effective_pool_a`.
  2. If `amount_b_required ≤ amount_b`, use `(amount_a, amount_b_required)` — the depositor offered enough B, so we take the full A and clamp B down.
  3. Otherwise, compute `amount_a_required = amount_b * effective_pool_a / effective_pool_b` and use `(amount_a_required, amount_b)` — B is the binding side, so we take the full B and clamp A down.
- All ratio math runs in `u128` with checked arithmetic. No floats are used for money; rounding is always toward the pool (the depositor never gets a sub-base-unit advantage).
- The ratio is computed on the **effective reserves** (`pool_X.amount - admin_fees_owed_X`). The admin's owed slice isn't LP-claimable capital, so it doesn't shift the deposit ratio.
- If the clamp rounds one of the amounts down to zero (e.g. a depositor offering a sub-base-unit fraction against a thick pool), the handler reverts with `DepositAmountTooSmall` rather than minting LP shares against a zero contribution.
- If the computed LP-token amount falls below `minimum_lp_tokens_out`, the handler reverts with `DepositBelowMinimum`. This is the depositor's slippage guard for cases where the pool ratio shifted between off-chain quote time and tx landing.

### `swap_tokens`

Swaps a fixed `input_amount` of one token for as much of the other as possible (subject to `min_output_amount`). The `input_is_token_a` flag selects the input side (`true` = trader sends token A and receives token B; `false` = the reverse).

- The total trading fee is taken off the input first: `fee_amount = input * fee / 10_000`.
- The fee is split between LPs and the admin:
  - `admin_portion = fee_amount * admin_share_bps / 10_000` — accumulates as a virtual claim on the input-side reserve (`admin_fees_owed_a` or `admin_fees_owed_b`). Not transferred immediately, swept later by `claim_admin_fees`. Saves a CPI per swap.
  - `lp_portion = fee_amount - admin_portion` — stays physically in the reserves and boosts LP yield ("less output for the same input").
- `taxed_input = input - fee_amount` is what enters the curve.
- The output is computed against the **effective reserves** (`pool_X.amount - admin_fees_owed_X`), so the admin's outstanding fees do not contribute to the price. The curve math runs in `u128` with checked arithmetic, multiplying before dividing to keep precision; floor rounding favours the pool (Uniswap V2 convention).
- If `output < min_output_amount`, the handler reverts with `SlippageExceeded`. This is the trader's slippage guard for cases where the pool shifted between quote time and tx landing.
- After the transfers, the handler reloads the pool accounts and re-verifies that `effective_pool_a * effective_pool_b` is at least as high as before the trade. This is defence in depth: if the curve math were ever wrong in a way that gave the trader too much, the invariant check would fail and revert the trade. Reverts with `InvariantViolated`.

### `withdraw_liquidity`

Burns LP tokens and returns a proportional share of the **effective reserves** (`pool_X.amount - admin_fees_owed_X`) to the LP. The proportion is `amount / (liquidity_provider_mint.supply + MINIMUM_LIQUIDITY)`. The admin's owed slice physically remains in the vaults but is not distributed to exiting LPs — it's claimed separately via `claim_admin_fees`. All math is `u128` with checked arithmetic, multiplying before dividing; floor rounding leaves sub-base-unit dust with the pool (grows LP value for everyone still in).

- `minimum_token_a_out` and `minimum_token_b_out` are the LP's per-side slippage floors. If either computed amount falls below its floor, the handler reverts with `WithdrawalBelowMinimum` *before* any tokens move. Pass `0` on either side to opt out. This protects LPs from withdrawing during a pool imbalance (e.g. a large swap landed just before this tx and skewed the mix).

### `claim_admin_fees`

Lets the address stored in `Config.admin` sweep their accumulated trading-fee claim out of a pool. Transfers `admin_fees_owed_a` from `pool_a` to the admin's token-A account and `admin_fees_owed_b` from `pool_b` to the admin's token-B account, signed by `pool_authority`. Then resets both accumulators to zero.

- Authorisation: enforced by Anchor's `has_one = admin` constraint on `config` plus the `Signer` constraint on `admin`. Calls from any other signer are rejected.
- The admin's token accounts (`admin_token_a`, `admin_token_b`) must already exist — this handler doesn't auto-create them (keeps the example small).
- Idempotent: calling again with the accumulators at zero is a successful no-op (transfers are skipped when owed = 0).

## Realistic lifecycle: an NVDAx/USDC pool

A worked example, end to end, using this program. Assume NVDAx (a tokenised NVIDIA share) trades around 5 USDC offchain.

**Cast:**

- **Anya** — deploys this AMM program to mainnet and runs it. Motivation: have a working AMM that people actually use, and earn fees from it. She calls `create_config` to fix the trading fee at 0.3% and sets `admin_share_bps = 1667` so she earns ~1/6 of every trading fee (LPs keep the other ~5/6). She also seeds the NVDAx/USDC pool herself (eating the locked `MINIMUM_LIQUIDITY` cost) so users have something to trade against from day one — which means she earns *twice*: her admin slice via `claim_admin_fees`, plus the LP yield on her initial deposit (same mechanism as Liam).
- **Liam** — yield farmer with idle USDC. Motivation: earn swap fees on his capital.
- **Trang** — retail trader. Motivation: get NVDAx exposure with USDC she already holds.
- **Sam** — arbitrageur. Motivation: make money on price gaps. (Side effect: his trades drag the pool's mid-price back toward the offchain market price, because that's where his profitable trade stops being profitable.)

### Step 1 — Anya creates the `Config`

The singleton `Config` account is set once per deployed program. Every pool inherits its `fee` and `admin_share_bps`.

- **Handler:** `create_config`
- **Accounts (`CreateConfigAccounts`):**
  - `config` (PDA, created) — seeds `[b"config"]`; stores `admin`, `fee`, `admin_share_bps`, `bump`
  - `admin` = Anya
  - `payer` = Anya
  - `system_program`
- **Args:** `fee = 30` (0.3%), `admin_share_bps = 1667` (Uniswap V2's classic 1/6 default — Anya keeps 1/6 of the trading fee; LPs keep 5/6)

`Config` exists. No pools yet, no liquidity yet.

### Step 2 — Anya creates the NVDAx/USDC pool

- **Handler:** `create_pool`
- **Accounts (`CreatePoolAccounts`):**
  - `config` — Anya's `Config`
  - `pool_config` (PDA, created) — seeds `[config, mint_a, mint_b]`; stores `config`, `mint_a`, `mint_b`, `bump`
  - `pool_authority` (PDA) — signs for the pool reserves
  - `liquidity_provider_mint` (created) — the LP-token mint, authority = `pool_authority`
  - `mint_a` = NVDAx mint, `mint_b` = USDC mint (with `mint_a < mint_b`)
  - `pool_a`, `pool_b` (created, ATAs owned by `pool_authority`) — the NVDAx and USDC reserves
  - `payer` = Anya
  - token, ATA, system programs
- **Args:** none

Pool exists; reserves are empty. No one can swap yet.

### Step 3 — Anya seeds initial liquidity

Anya picks a 1:5 ratio so the pool launches at ~5 USDC per NVDAx. She deposits **20 NVDAx and 100 USDC**.

- **Handler:** `deposit_liquidity`
- **Accounts (`DepositLiquidityAccounts`):**
  - `pool_config`, `pool_authority`, `liquidity_provider_mint`
  - `depositor` = Anya (signer)
  - `mint_a`, `mint_b`
  - `pool_a`, `pool_b` (the pool's reserves)
  - `liquidity_provider_token` — Anya's LP-token ATA (created)
  - `token_a` — Anya's NVDAx ATA, `token_b` — Anya's USDC ATA
  - `payer` = Anya
  - token, ATA, system programs
- **Args:** `amount_a = 20`, `amount_b = 100`, `minimum_lp_tokens_out = 0` (initial deposit — Anya is the only LP, no slippage risk; production code should still set a floor to guard against frontrun pool-creations)

Math:

- LP tokens minted on the first deposit: `sqrt(20 × 100) = sqrt(2000) ≈ 44.72`.
- Minus the locked `MINIMUM_LIQUIDITY = 100` floor (base units — negligible at major-unit scale).
- Anya receives ~44.72 LP tokens. The 100 base-unit dust is locked forever, owned by no one. Anya eats that cost as the price of bootstrapping.

Pool state: **20 NVDAx, 100 USDC**. Mid-price = 5. Anya owns 100% of withdrawable LP supply.

### Step 4 — Liam adds liquidity

At the current 1:5 ratio, Liam deposits **100 NVDAx and 500 USDC**.

- **Handler:** `deposit_liquidity`
- **Accounts:** same shape as Step 3, `depositor` = Liam
- **Args:** `amount_a = 100`, `amount_b = 500`, `minimum_lp_tokens_out = 223_000_000` (Liam quoted ~223.6 LP off-chain and is unwilling to accept less than ~223.0 if the pool shifts before his tx lands; units here are LP base units at the LP mint's decimals)

Math: subsequent deposits get `min(amount_a / pool_a, amount_b / pool_b) × current_lp_supply = min(100/20, 500/100) × 44.72 ≈ 223.6` LP tokens.

Pool state: **120 NVDAx, 600 USDC**. LP supply ~268.32. Liam owns ~83%, Anya ~17%.

### Step 5 — Trang buys NVDAx with USDC

- **Handler:** `swap_tokens`
- **Accounts (`SwapTokensAccounts`):**
  - `config` — for the fee
  - `pool_config`, `pool_authority`
  - `trader` = Trang (signer)
  - `mint_a`, `mint_b`
  - `pool_a`, `pool_b` (the pool's reserves)
  - `token_a` — Trang's NVDAx ATA (created if missing), `token_b` — Trang's USDC ATA
  - `payer` = Trang
  - token, ATA, system programs
- **Args:** `input_is_token_a = false` (input is token B = USDC), `input_amount = 11`, `min_output_amount = 1.9`

Math (constant product, 0.3% fee from `Config.fee`, fee split per `Config.admin_share_bps`):

- Total fee on the input: `11 × 0.003 = 0.033 USDC`.
- Fee split:
  - Admin slice (`admin_share_bps = 1667`): `0.033 × 0.1667 ≈ 0.0055 USDC` — added to `admin_fees_owed_b`.
  - LP slice: `0.033 − 0.0055 ≈ 0.0275 USDC` — stays in the reserves, boosts LP yield.
- Input into the curve: `11 − 0.033 = 10.967 USDC`.
- Effective reserves before the trade: `effective_pool_a = 120`, `effective_pool_b = 600` (admin owes nothing yet).
- New effective B: `600 + 10.967 = 610.967` (raw `pool_b.amount` is `611`, minus the new admin slice `0.0055`).
- New effective A: `(120 × 600) / 610.967 ≈ 117.844`.
- NVDAx out: `120 − 117.844 ≈ 2.156`.

Trang gets ~2.156 NVDAx. Effective price ~5.10 USDC/NVDAx — worse than mid-price because of the fee plus her own price impact.

Pool state: **117.844 NVDAx, 611 USDC raw** (`admin_fees_owed_a = 0`, `admin_fees_owed_b ≈ 0.0055`). Mid-price on the effective reserves drifted up to ~5.18.

### Step 6 — Sam arbitrages

NVDAx still trades at 5.00 offchain; our pool now says 5.18. There's a profitable trade: buy NVDAx offchain at 5.00, sell it into the pool at ~5.18. Sam does it.

- **Handler:** `swap_tokens`
- **Accounts:** same shape as Step 5, `trader` = Sam
- **Args:** `input_is_token_a = true` (input is token A = NVDAx), `input_amount = 2.15`, `min_output_amount = 10.5`

Math:

- Total fee on the input: `2.15 × 0.003 ≈ 0.00645 NVDAx`.
- Fee split:
  - Admin slice: `0.00645 × 0.1667 ≈ 0.001075 NVDAx` — added to `admin_fees_owed_a`.
  - LP slice: `≈ 0.005375 NVDAx` — stays in the reserves.
- Input into the curve: `2.15 − 0.00645 ≈ 2.14355 NVDAx`.
- Effective reserves before the trade: `effective_pool_a = 117.844` (no A-side admin claim yet), `effective_pool_b ≈ 611 − 0.0055 ≈ 610.9945`.
- New effective A: `117.844 + 2.14355 ≈ 119.9876`.
- New effective B: `(117.844 × 610.9945) / 119.9876 ≈ 600.07`.
- USDC out: `610.9945 − 600.07 ≈ 10.92`.

Sam paid ~10.75 USDC offchain for 2.15 NVDAx, sold into the pool for ~10.92 USDC. Profit ~0.17 USDC, minus gas.

Pool state: **119.987 NVDAx, 600.07 USDC raw**, with `admin_fees_owed_a ≈ 0.001075` and `admin_fees_owed_b ≈ 0.0055`. Mid-price on the effective reserves back to ~5.00 — *because* that's the price at which Sam's profit hit zero and he stopped.

### Step 8 — Anya claims her admin fees

After a month of trading activity, Anya sweeps her accumulated slice.

- **Handler:** `claim_admin_fees`
- **Accounts (`ClaimAdminFeesAccounts`):**
  - `config` — Anya's `Config` (the `has_one = admin` constraint enforces that only she can call this)
  - `pool_config`, `pool_authority`
  - `mint_a`, `mint_b`
  - `pool_a`, `pool_b` (the pool's reserves — the source of the transfers)
  - `admin` = Anya (signer)
  - `admin_token_a` — Anya's NVDAx ATA (must already exist)
  - `admin_token_b` — Anya's USDC ATA (must already exist)
  - `token_program`
- **Args:** none

She receives her accumulated `admin_fees_owed_a` of NVDAx and `admin_fees_owed_b` of USDC. Both accumulators reset to zero on the same instruction. From this example's two swaps that's only `~0.001075 NVDAx` and `~0.0055 USDC` — small, because the fee is small and only two trades have happened, but real volume would compound it.

Pool state: **119.986 NVDAx, 600.065 USDC raw**, with `admin_fees_owed_a = 0` and `admin_fees_owed_b = 0`.

### Step 9 — Liam withdraws

Later on, Liam exits.

- **Handler:** `withdraw_liquidity`
- **Accounts (`WithdrawLiquidityAccounts`):** same shape as deposit, `depositor` = Liam
- **Args:** `amount ≈ 223.6` (burn all his LP tokens); `minimum_token_a_out` and `minimum_token_b_out` set to his quoted-out amounts minus a small tolerance, so a sandwich swap before his tx lands can't drain one side below his floor without reverting his exit

He receives his proportional share of the **effective reserves** (`pool_X.amount - admin_fees_owed_X`), so Anya's accumulated slice doesn't dilute his withdrawal. Because the effective reserves grew faster than LP supply (LP supply only changes on deposits and withdrawals; the LP slice of every fee accrues into the effective reserves), he gets back slightly more NVDAx and slightly more USDC than he put in. The difference is his fee income.

### Recap

- **Anya** calls `create_config` → `create_pool` → `deposit_liquidity` (admin, pool creator, initial LP)
- **Liam** calls `deposit_liquidity` (LP)
- **Trang** calls `swap_tokens` with `input_is_token_a = false` (buyer)
- **Sam** calls `swap_tokens` with `input_is_token_a = true` (arbitrageur)
- **Anya** calls `claim_admin_fees` (sweep her accumulated fee slice)
- **Liam** later calls `withdraw_liquidity` (exit)

What makes this work: `x × y = K` on the effective reserves keeps the pool solvent on every swap without anyone quoting prices. LPs are paid in growing effective reserves (their share of the fee, parameterised by `Config.fee` and `Config.admin_share_bps`); the admin earns the other share, accumulated lazily and swept on demand; profit-chasing arbitrageurs incidentally keep the mid-price honest; traders get instant fills against a passive counterparty (the pool).

## Tests

Run `cargo test` from the `anchor/` directory (or `anchor test`, which `Anchor.toml` wires to the same command). Tests are Rust + LiteSVM, in `programs/token-swap/tests/`.
