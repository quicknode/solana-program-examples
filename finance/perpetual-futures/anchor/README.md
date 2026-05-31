# Perpetual Futures

A perpetual futures exchange — a venue for making leveraged bets on an asset's price without ever owning the asset. It is modelled on the oracle-priced, pool-collateralized design used by [Jupiter Perpetuals](https://station.jup.ag/guides/perpetual-exchange/overview) and GMX (and the open-source [`solana-labs/perpetuals`](https://github.com/solana-labs/perpetuals) reference that [Adrena](https://github.com/AdrenaFoundation/adrena-program) and [Flash Trade](https://github.com/flash-trade/flash-perpetuals) fork), rather than the order-book design used by [Drift](https://docs.drift.trade/).

The collateral is **USDC** (a dollar stablecoin), and the market tracks the price of **NVDAx**, a tokenised Nvidia share whose [oracle](#oracle) price follows the real stock. A second market could track **TSLAx** (Tesla); each market is one collateral token plus one price feed. In the tests these are mock [SPL tokens](https://solana.com/docs/terminology#token).

A [perpetual future](https://www.investopedia.com/terms/f/futurescontract.asp) ("perp") is a [derivative](https://www.investopedia.com/terms/d/derivative.asp) with no expiry: profit and loss is paid in the collateral token as the price moves, and no stock or coin ever changes hands.

[⚓ Anchor](.) · [💫 Quasar](../quasar)

---

## Programs

| Program | Description |
|---------|-------------|
| `perpetual-futures` | The exchange: pool creation, liquidity provision, opening/closing leveraged positions, funding, liquidation, and fee collection. |
| `mock-switchboard` | Test-only price feed. Stores a price, scale, last-update slot, and confidence band that tests write directly. Replaced by a real [Switchboard](https://docs.switchboard.xyz/) On-Demand feed in production. |

All money math is integer `u128` with `checked_*` operations, multiplying before dividing and rounding in the pool's favour — no floats, no fixed-point library.

---

## Key Financial Concepts

### Long and short, leverage, collateral

A trader goes [long](https://www.investopedia.com/terms/l/long.asp) if they think the price will rise or [short](https://www.investopedia.com/terms/s/short.asp) if they think it will fall. They post [collateral](https://www.investopedia.com/terms/c/collateral.asp) and choose a position size up to the pool's maximum [leverage](https://www.investopedia.com/terms/l/leverage.asp) (borrowing power). The [notional size](https://www.investopedia.com/terms/n/notionalvalue.asp) is the full exposure — e.g. $5,000 even if only $1,000 of collateral was posted — and profit or loss is the notional times the percentage change in price:

```
long  profit/loss = size * (price - entry_price) / entry_price
short profit/loss = size * (entry_price - price) / entry_price
```

### The liquidity pool and provider shares

There is no order book. Every trade is against one shared [liquidity pool](https://www.investopedia.com/terms/l/liquidity.asp) that other users fund; the pool is the counterparty to all of them — it pays trader profits and keeps trader losses. Providers receive shares priced against [mark-to-market](https://www.investopedia.com/terms/m/marktomarket.asp) assets-under-management (the pool's value if every open position were settled now), derived from running per-side accumulators rather than by iterating positions. Pricing against the marked value stops a provider exiting just before an in-flight trader profit is realized. The first deposit mints `deposit - MINIMUM_LIQUIDITY` shares (the Uniswap V2 convention) so the share supply never starts at a dust amount.

### Reserved liquidity

So a winning trader can always be paid, the pool **reserves** liquidity to back each open position's maximum recoverable profit (its notional `size`). An open is allowed only while `reserved + size <= liquidity`, which doubles as an open-interest cap. `close_position` caps a winner's payout at the reserved `size` (for a long, profit is capped on a more-than-doubling move; a short's profit is naturally within `size`), and provider withdrawals can take only the *free* remainder (`liquidity - reserved`). This is the simplified, single-collateral form of the reserve accounting in `solana-labs/perpetuals`. The reserve covers price profit only — funding owed *to* a position (the lighter side receives funding) is not reserved, so in the extreme a payout the pool cannot cover makes the close fail closed (revert) rather than leave the pool insolvent.

### Funding

[Funding](https://www.investopedia.com/terms/f/futurescontract.asp) anchors the pool's risk: the heavier side of [open interest](https://www.investopedia.com/terms/o/openinterest.asp) pays the pool over time. A cumulative funding index rises while longs are the larger side and falls while shorts are, advancing by `funding_rate_per_slot` each [slot](https://solana.com/docs/terminology#slot); a position records the index at open and settles the change when it closes. In a pool-based perp this is the equivalent of the borrow fee Jupiter Perpetuals charges.

### Maintenance margin and liquidation

A position's *equity* is its net collateral plus profit/loss minus funding. Once equity falls to or below the [maintenance margin](https://www.investopedia.com/terms/m/maintenancemargin.asp) (`maintenance_margin_bps` of notional), the position can be [liquidated](https://www.investopedia.com/terms/l/liquidation.asp). Liquidation is permissionless — anyone can crank it and earn the liquidation fee.

### Oracle

The mark price comes from an oracle feed. This example validates the price for staleness (by slot), positivity, scale, and a [confidence band](https://docs.pyth.network/price-feeds/best-practices#confidence-intervals) that must stay within `max_confidence_bps` of the price — rejecting an uncertain price is one of the most common oracle-safety checks.

### Fees and slippage

Open and close fees are charged in [basis points](https://www.investopedia.com/terms/b/basispoint.asp) (1 bp = 0.01%) of notional and accrue to the protocol. Every state-changing handler takes a `minimum_*` / acceptable-price bound — protection against [slippage](https://www.investopedia.com/terms/s/slippage.asp), the gap between the expected and actual fill — and reverts if the bound is breached. Pass `0` to opt out.

---

## Program Flow

### Participants

| Person | Role | Motivation |
|--------|------|-----------|
| **Admin** | Pool authority | Operate the market and collect the protocol's slice of trading fees. |
| **Carol** | Liquidity provider | Earn fees by funding the pool and being the counterparty to traders. |
| **Alice** | Long trader | She has a thesis that NVDA will rise and wants leveraged upside without buying the stock. |
| **Bob** | Short trader | He thinks NVDA will fall and wants to profit from the downside. |
| **Dave** | Liquidator | Runs a bot that closes under-margined positions to earn the liquidation fee. |

Amounts below are shown in whole USDC; on-chain they are base units (× 10⁶). The pool is configured with 10× max leverage, 0.1% open/close fees, a 5% maintenance margin, a 1% liquidation fee, and a 1% maximum oracle confidence band.

---

### Step 1 — Admin opens the market

**Instruction:** `initialize_pool(parameters)`

**Accounts created:**

| Account | Seeds / Derivation | What it stores |
|---------|--------------------|----------------|
| `Pool` [PDA](https://solana.com/docs/terminology#program-derived-address-pda) | `["pool", collateral_mint, oracle_feed]` | parameters, liquidity, reserved liquidity, collateral total, per-side open-interest accumulators, funding index, protocol fees |
| `pool_authority` PDA | `["authority", pool]` | nothing; signs vault and mint CPIs |
| `custody_vault` [token account](https://solana.com/docs/terminology#token-account) PDA | `["vault", pool]` | all USDC — both provider liquidity and trader collateral |
| `lp_mint` PDA | `["lp_mint", pool]` | the share [mint](https://solana.com/docs/terminology#mint-account); `pool_authority` is the mint authority |

---

### Step 2 — Carol provides liquidity

**Instruction:** `add_liquidity(amount = 100_000 USDC, minimum_shares_out)`

**Accounts modified:**

| Account | Change |
|---------|--------|
| `carol_usdc` | −100,000 USDC |
| `custody_vault` | +100,000 USDC |
| `lp_mint` → `carol_lp` (created) | mints ≈100,000 shares to Carol |
| `Pool.liquidity` | 0 → 100,000 |

The pool can now pay trader winnings, and Carol holds shares representing her slice of it.

---

### Step 3 — Alice opens a 5× long

**Instruction:** `open_position(side = Long, collateral_amount = 1,000 USDC, size = 5,000 USDC, acceptable_price)`

NVDAx is at $100. The 0.1% open fee ($5) comes out of her collateral, leaving $995 of net collateral backing the position.

**Accounts modified:**

| Account | Change |
|---------|--------|
| `Position` PDA `["position", pool, alice, Long]` (created) | side Long, collateral $995, size $5,000, entry price $100 |
| `alice_usdc` | −1,000 USDC |
| `custody_vault` | +1,000 USDC |
| `Pool.total_collateral` | +$995 |
| `Pool.protocol_fees` | +$5 |
| `Pool.reserved_liquidity` | +$5,000 (must stay ≤ liquidity) |
| `Pool` long open-interest accumulators | += this position |

---

### Step 4 — Bob opens a 5× short

**Instruction:** `open_position(side = Short, collateral_amount = 1,000 USDC, size = 5,000 USDC, acceptable_price)`

**Accounts modified:** a `Position` PDA `["position", pool, bob, Short]` is created; `custody_vault` +1,000 USDC; `Pool.total_collateral` +$995; `Pool.protocol_fees` +$5; `Pool.reserved_liquidity` +$5,000 (now $10,000 of the $100,000 reserved); short open-interest accumulators rise.

While both are open, **funding** accrues to the pool from the heavier side; it is settled when each position closes.

---

### Step 5 — NVDA rises to $116. Alice closes in profit

**Instruction:** `close_position(minimum_payout)`

Her profit is `5,000 × (116 − 100) / 100 = $800` (well under the $5,000 reserve cap), minus the $5 close fee.

**Accounts modified:**

| Account | Change |
|---------|--------|
| `Pool.liquidity` | −$800 (providers pay her profit) |
| `Pool.reserved_liquidity` | −$5,000 (reserve released) |
| `Pool.total_collateral` | −$995 |
| `Pool.protocol_fees` | +$5 |
| long open-interest accumulators | −= this position |
| `custody_vault` → `alice_usdc` | pays out $1,790 (net collateral + profit − close fee) |
| `Position` (Alice) | closed; rent returned to Alice |

---

### Step 6 — Bob's short is underwater. Dave liquidates it

**Instruction:** `liquidate_position()`

At $116 Bob's short has lost $800; his equity ($995 − $800 = $195) has fallen below the 5% maintenance margin ($250), so anyone may close it.

**Accounts modified:**

| Account | Change |
|---------|--------|
| short open-interest accumulators | −= Bob's position |
| `Pool.reserved_liquidity` | −$5,000 (reserve released) |
| `Pool.total_collateral` | −$995 |
| `Pool.liquidity` | +$800 (the loss accrues to providers) |
| `custody_vault` → `dave_usdc` (created) | $50 liquidation fee |
| `custody_vault` → `bob_usdc` | $145 remaining equity refunded |
| `Position` (Bob) | closed; rent returned to Bob |

---

### Step 7 — Admin collects the protocol's fees

**Instruction:** `collect_fees()`

**Accounts modified:** `Pool.protocol_fees` → 0; `custody_vault` pays that amount to `admin_usdc`.

---

### Step 8 — Carol withdraws

**Instruction:** `remove_liquidity(shares, minimum_amount_out)`

Carol burns her shares and redeems USDC. Her balance now reflects the fees the pool earned plus the net of traders' wins and losses while she was in. She can withdraw only the *free* liquidity — while a position is open, the part backing it is reserved and cannot be pulled out.

**Accounts modified:** `lp_mint` burns Carol's shares; `Pool.liquidity` falls; `custody_vault` pays out USDC to `carol_usdc`.

---

## Design notes and further reading

The genuinely hard part of a perpetual-futures venue is keeping it solvent and permissionless *without* re-evaluating the entire market on every action. For a rigorous, formally-verified (Kani) treatment, see Anatoly Yakovenko's [percolator](https://github.com/aeyakovenko/percolator), an educational perp risk engine. It states three invariants this example also leans on, in simplified form:

- **Realizable credit** — "protected principal is senior, positive PnL is junior, and source-domain positive credit cannot exceed realizable backing reserved for that domain." Here, provider capital is senior and trader profit is a junior claim against it: shares are priced against marked assets-under-management, and the pool reserves each position's payout up front (capping recoverable profit at the reserve) so a winner's price profit can always be paid.
- **Account-local safety** — "every favorable action refreshes the account's full active portfolio first; … stale … legs fail closed." Here, every position and liquidity action reads a fresh oracle (stale or wide-confidence prices are rejected) and recomputes pool exposure before any payout.
- **Bounded progress** — "no public instruction needs to evaluate the whole market." Here, assets-under-management comes from running per-side accumulators, and liquidation acts on one position at a time, so no handler's cost grows with the number of open positions.

What production pool-perps (`solana-labs/perpetuals`) add that this example still leaves out: multi-asset custody with reserves in the payout token, utilization-based borrow fees, auto-deleveraging (ADL) and an insurance fund for the bad-debt tail, and using the oracle's EMA for a less manipulable mark.

---

## Limitations

This is a teaching example, not an audited exchange. Notably:

- A single position per side per trader, and one collateral token per pool.
- Recoverable profit is capped at the reserved notional, so the cap binds on a more-than-doubling move; a production venue would let profit run and absorb extreme moves with ADL, an insurance fund, and bankruptcy-residual accounting.
- The liquidation reward is paid from the position's remaining equity, so a position that gaps straight through zero equity pays the liquidator nothing — production venues fund the reward from collateral or an insurance fund so the worst positions are still worth liquidating.
- Funding is a single time-decay index on the heavier side rather than a skew-weighted rate.

---

## Testing

The tests run in-process with [LiteSVM](https://www.anchor-lang.com/docs/testing/litesvm) and [solana-kite](https://solanakite.org); no local validator is needed. They deploy both programs, drive the mock oracle, and cover liquidity round-trips, opening and closing longs and shorts in profit and loss, leverage and slippage rejection, stale-price and wide-confidence rejection, funding accrual, liquidation (and the refusal to liquidate a healthy position), reserved-liquidity behaviour (profit capped at the reserve, opens rejected when the pool can't back them, withdrawals blocked by reserved liquidity), and fee collection.

```bash
anchor build
cargo test --manifest-path programs/perpetual-futures/Cargo.toml
```

`anchor build` first, so the LiteSVM tests can load each program's compiled `.so` via `include_bytes!`.
