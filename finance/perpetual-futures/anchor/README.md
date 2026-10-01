# Solana Perpetual Futures (Anchor)

> [!NOTE]
> This is the **Anchor v2** copy of this example. Every `anchor` command on this page
> needs the v2 CLI: `cargo install anchor-cli --version 2.0.0-rc.1 --locked` (avm has
> no prebuilt binary for this pre-release). The Anchor v1 version of this example is in
> [`../anchor-v1`](../anchor-v1/).

A perpetual futures exchange on Solana: a venue for making leveraged bets on an asset's price without ever owning the asset. It is modelled on the oracle-priced, pool-collateralized design used by [Jupiter Perpetuals](https://station.jup.ag/guides/perpetual-exchange/overview) and GMX (and the open-source [`solana-labs/perpetuals`](https://github.com/solana-labs/perpetuals) reference that [Adrena](https://github.com/AdrenaFoundation/adrena-program) and [Flash Trade](https://github.com/flash-trade/flash-perpetuals) fork), rather than the order-book design used by [Drift](https://docs.drift.trade/).

The collateral is **USDC** (a dollar stablecoin), and the market tracks the price of **NVDAx**, a tokenised Nvidia share whose [oracle](#oracle) price follows the real stock. A second market could track **TSLAx** (Tesla); each market is one collateral token plus one price feed. In the tests these are mock [SPL tokens](https://solana.com/docs/terminology#token).

A [perpetual future](https://www.investopedia.com/terms/f/futurescontract.asp) ("perp") is a [derivative](https://www.investopedia.com/terms/d/derivative.asp) with no expiry: profit and loss is paid in the collateral token as the price moves, and no stock or coin ever changes hands.

[⚓ Anchor](.) · [💫 Quasar](../quasar)

---

## Programs

- `perpetual-futures`: The exchange: pool creation, liquidity provision, opening/closing leveraged positions, funding, liquidation, and fee collection.
- `mock-price-feed`: Test-only price feed. Stores a price, scale, last-update slot, and confidence band that tests write directly. Replaced in production by a Pyth `PriceUpdateV2` account, as read in [`basics/pyth`](../../../basics/pyth/).

All arithmetic is integer `u128` with `checked_*` operations, multiplying before dividing and rounding in the pool's favour: no floats, no fixed-point library.

---

## Key Financial Concepts

### Long and short, leverage, collateral

A trader goes [long](https://www.investopedia.com/terms/l/long.asp) if they think the price will rise or [short](https://www.investopedia.com/terms/s/short.asp) if they think it will fall. They post [collateral](https://www.investopedia.com/terms/c/collateral.asp) and choose a position size, and the pool's [initial margin](https://www.investopedia.com/terms/i/initialmargin.asp) caps their [leverage](https://www.investopedia.com/terms/l/leverage.asp) (borrowing power): `open_position` requires the collateral left after the open fee to be at least `initial_margin_bps` of the size, checked as `net_collateral * 10_000 >= size * initial_margin_bps`, and fails with `InitialMarginNotMet` otherwise. An initial margin of 1,000 basis points (10%) allows at most 10× leverage. The [notional size](https://www.investopedia.com/terms/n/notionalvalue.asp) is the full exposure (e.g. $5,000 even if only $1,000 of collateral was posted) and profit or loss is the notional times the percentage change in price:

```
long  profit/loss = size * (price - entry_price) / entry_price
short profit/loss = size * (entry_price - price) / entry_price
```

### The liquidity pool and provider shares

There is no order book. Every trade is against one shared [liquidity pool](https://www.investopedia.com/terms/l/liquidity.asp) that other users fund; the pool is the counterparty to all of them: it pays trader profits and keeps trader losses. Providers receive shares priced against [mark-to-market](https://www.investopedia.com/terms/m/marktomarket.asp) assets-under-management (the pool's value if every open position were settled now), derived from running per-side accumulators rather than by iterating positions. Pricing against the marked value stops a provider exiting just before an in-flight trader profit is realized. The first deposit mints `deposit - MINIMUM_LIQUIDITY` shares (the Uniswap V2 convention) so the share supply never starts at a dust amount, and both `add_liquidity` and `remove_liquidity` divide by the share supply plus `MINIMUM_LIQUIDITY`, so the withheld shares belong to nobody and their slice of the pool never leaves. That lock is what defeats share inflation here. Tokens sent straight to the vault move nothing, because shares are priced against `Pool.liquidity`, but `liquidity` grows with funding payments and trader losses, and a provider can also be the pool's only trader. An attacker holding one share who pays funding into the pool to make each share expensive owns 1 of 1,001 shares, so almost all of what they pay in stays with the withheld minimum.

### Profit is paid as far as the pool can back it: the haircut

The risk model comes from Anatoly Yakovenko's [Percolator](https://github.com/aeyakovenko/percolator): a trader's collateral is **senior**, and their profit is **junior**, paid only as far as the pool holds the tokens to pay it. Nothing is set aside when a position opens, the pool's liquidity does not limit how large a position can be, and profit has no cap. The pool stays solvent at exit instead. When `close_position` settles a winning position it first computes the **haircut ratio** `h`:

```
backing   = liquidity + insurance_fund
liability = max(0, traders' aggregate unrealized profit, closing position's profit)
h         = min(1, backing / liability)
```

The liability comes from the same per-side accumulators that price provider shares, at the current price and before the closing position leaves them, so no handler iterates positions. While the backing covers the liability, `h` is one and every profit is paid in full. When a sharp move leaves traders owed more than the backing, every winner who closes is paid `profit * h`, rounded down, so each is paid the same fraction of their profit. The part a haircut withholds stays in `liquidity`, and `h` rises again as losing positions settle their losses into the pool. A loss is never haircut. `HAIRCUT_PRECISION` (10⁹) is the fixed point `h` is carried in.

Open losing positions offset winners in the aggregate, so one winner's profit can be larger than what traders are owed in total. That is why the closing position's own profit is in the `max`: a winner who closes while open losers still offset them is paid at most the pool's backing, and the close is never refused for lack of it. Whenever the aggregate is the larger of the two, the closer's own profit changes nothing, and every other winner's fraction is unchanged.

The profit is paid from `liquidity` first. If it is larger than `liquidity`, the insurance fund pays the rest, since the haircut counted the fund as backing. Because the haircut keeps the profit within both, `PoolInsolvent` remains only as a defensive check.

`test_haircut_scales_profit_when_pool_stressed` opens two longs owed $1,800 between them against $900 of liquidity and checks that the first to close and the second are each paid exactly half of their profit, `test_insurance_pays_profit_beyond_liquidity` checks a profit larger than `liquidity` is paid in full with the insurance fund covering the difference, and `test_winner_offset_by_open_loser_is_paid_not_refused` closes a long up $1,000 while a short down $900 is still open, against $300 of backing, and checks the long is paid exactly $300 and the short's later close settles its loss in full.

### Profit warm-up

Every position records the slot it opened in, `Position.entry_slot`. `close_position` refuses to pay a profit before slot `entry_slot + profit_warmup_slots`, failing with `ProfitNotMatured`, so someone who pushes the oracle to a false price cannot open a position and take its profit less than `profit_warmup_slots` apart; by then the price has had that long to correct. A losing position can close in the slot it opened, and liquidation is never delayed. `profit_warmup_slots` is fixed by `initialize_pool`.

### The insurance fund

`insurance_fee_bps` of every open and close fee goes to `Pool.insurance_fund`, rounded down, and the rest to `Pool.program_fees`, so the two add up to the whole fee. `initialize_pool` refuses an `insurance_fee_bps` of 10,000 or more with `InvalidParameter`. The fund never pays a fee. It pays for two things:

- When a liquidated position's equity is below zero, it lost more than its collateral. The fund pays that deficit as far as it can, and the liquidity providers bear only the rest.
- It pays a winner's profit once `liquidity` is exhausted, as above.

The vault always holds `liquidity + total_collateral + program_fees + insurance_fund`, plus any tokens sent to it directly; the tests' `assert_vault_matches_ledger` checks that after the haircut, insurance-fund and withdrawal scenarios.

Provider withdrawals are capped at `liquidity`. Shares are priced against assets-under-management, which counts traders' unrealized losses as the providers' gain, but those losses are still in the traders' collateral until their positions close, so `remove_liquidity` fails with `InsufficientLiquidity` when a redemption would pay out more than `liquidity`. While traders are up instead, share pricing already keeps a withdrawal below `liquidity` minus their profit, so the backing for that profit stays in the pool.

### Funding

[Funding](https://www.investopedia.com/terms/f/futurescontract.asp) anchors the pool's risk: the heavier side of [open interest](https://www.investopedia.com/terms/o/openinterest.asp) pays the pool over time. A cumulative funding index rises while longs are the larger side and falls while shorts are, advancing by `funding_rate_per_second` for each second on the Clock's `unix_timestamp`; a position records the index at open and settles the change when it closes. In a pool-based perp this is the equivalent of the borrow fee Jupiter Perpetuals charges.

Funding runs on the wall clock rather than the slot count, so what a position costs per hour is set by the rate alone and does not change when the cluster's slot time does. The timestamp is written by each block's leader, but the runtime bounds how far one block can move it, so the elapsed time behind a position's funding is out by a second or two at most; a timestamp at or before the stored `last_funding_timestamp` accrues nothing. The rate is set once, in `initialize_pool`, and cannot be changed afterwards; `initialize_pool` refuses a rate above `MAX_FUNDING_RATE_PER_SECOND` (277, just under 0.1% of a position's size per hour). The lighter side is paid funding out of `liquidity`, so an operator who could raise the rate at will could hold a small position on that side, raise the rate and close it to take the liquidity providers' deposits. `test_operator_on_the_lighter_side_earns_only_the_fixed_rate` runs that position and checks it earns only the fixed rate.

### Maintenance margin and liquidation

A position's *equity* is its net collateral plus profit/loss minus funding. Once equity falls to or below the [maintenance margin](https://www.investopedia.com/terms/m/maintenancemargin.asp) (`maintenance_margin_bps` of notional), the position can be [liquidated](https://www.investopedia.com/terms/l/liquidation.asp). Liquidation is permissionless: anyone can crank it and earn the liquidation fee, `liquidation_fee_bps` of the position's size, paid out of its remaining equity. Whatever part of the fee the equity cannot cover is forgiven, as in Percolator: neither the insurance fund nor the liquidity providers pay it, so a liquidator of a position whose equity is already below zero receives nothing, and the position still closes. The insurance fund pays its deficit first (see [the insurance fund](#the-insurance-fund)); `test_liquidation_of_bankrupt_position_charges_insurance_before_liquidity` liquidates such a position and checks the exact split.

`initialize_pool` requires `maintenance_margin_bps < initial_margin_bps <= 10_000` and refuses anything else with `InitialMarginNotAboveMaintenance` (or `InvalidParameter` above 10,000). Every position therefore opens with more margin than it is liquidated at, so none can be liquidated in the slot it opened.

### Oracle

The mark price comes from an oracle feed. This example validates the price for staleness (by slot), publication after the most recent cluster restart (the `LastRestartSlot` sysvar, because a halt passes hours of wall-clock time in zero slots), positivity, scale, and a [confidence band](https://docs.pyth.network/price-feeds/best-practices#confidence-intervals) that must stay within `max_confidence_bps` of the price: rejecting an uncertain price is one of the most common oracle-safety checks.

### Price band

A single oracle print can be wrong while still being fresh, positive and confident: a publisher fault, or a thin market moved for a few seconds. To stop anyone trading against such a print, the pool keeps its own time-weighted moving average of the oracle price, `Pool.average_price`, and refuses prices too far from it.

- `initialize_pool` reads the oracle and seeds both `average_price` and `last_oracle_price` with its price, stamping `average_price_timestamp` with the Clock's `unix_timestamp`.
- Every handler that reads the oracle credits the seconds since the previous read to the price that read saw, `last_oracle_price`, on the assumption that it held throughout: `average += (last_oracle_price - average) * min(elapsed, PRICE_AVERAGE_WINDOW_SECONDS) / PRICE_AVERAGE_WINDOW_SECONDS`. It then records the price it read as the new `last_oracle_price`. The window is 600 seconds, so a price seen at two reads six seconds apart moves the average by 1% of its gap from the average, and an interval of ten minutes or more replaces the average with the price seen at its start.
- The price read now only starts counting from now. A manipulated price moves the average only if the oracle still shows it at a later read, and only by the seconds between the two reads; a read of the real price in between replaces it. A pool left idle for longer than the window therefore cannot have its average set by a single read.
- `open_position`, `close_position`, `add_liquidity` and `remove_liquidity` first check the price against the stored average, before anything is folded in: `|price - average_price| * 10_000 <= average_price * max_price_deviation_bps`. A price outside that band fails with `PriceOutsideBand`, and the pool is left unchanged.
- `liquidate_position` folds and records without the band check. A genuine crash is when positions go underwater, so liquidation keeps working through one.
- `update_price_average()` is permissionless: any signer passes the pool and its oracle feed, and the handler reads and validates the oracle with the same checks, accrues funding, folds the elapsed interval in and records the price, with no band check. After a genuine move takes the oracle outside the band, keepers call it repeatedly as time passes: the first call records the new price, and each later call credits the time since the previous one to it, until the average is close enough to the price for trading to resume.

`max_price_deviation_bps` is fixed by `initialize_pool`, which refuses zero (every move would be refused) and 10,000 or more (a fall could never be refused, since prices are positive) with `InvalidPriceDeviation`.

### Fees and slippage

Open and close fees are charged in [basis points](https://www.investopedia.com/terms/b/basispoint.asp) (1 bp = 0.01%) of notional; `insurance_fee_bps` of each goes to the insurance fund and the rest accrues to the program. Every state-changing handler takes a `minimum_*` / acceptable-price bound (protection against [slippage](https://www.investopedia.com/terms/s/slippage.asp), the gap between the expected and actual fill) and reverts if the bound is breached. Pass `0` to opt out.

---

## Program Flow

### Participants

- **Admin** (Pool operator): Operate the market and collect the program's slice of trading fees.
- **Carol** (Liquidity provider): Earn fees by funding the pool and being the counterparty to traders.
- **Alice** (Long trader): She has a thesis that NVDA will rise and wants leveraged upside without buying the stock.
- **Bob** (Short trader): He thinks NVDA will fall and wants to profit from the downside.
- **Dave** (Liquidator): Runs a bot that closes under-margined positions to earn the liquidation fee.

Amounts below are shown in whole USDC; onchain they are base units (× 10⁶). The pool is configured with a 10% initial margin (10× leverage), 0.1% open/close fees with half of each paid into the insurance fund, a 5% maintenance margin, a 1% liquidation fee, a 1% maximum oracle confidence band, a 20% price band around its average price, and a 10-slot profit warm-up.

---

### Step 1: Admin opens the market

**Instruction:** `initialize_pool(parameters)`

The handler validates the parameters, then reads the oracle once to seed the pool's average price at $100.

**Accounts created:**

- `Pool` [PDA](https://solana.com/docs/terminology#program-derived-address-pda), seeds `["pool", collateral_mint, oracle_feed]`: parameters, liquidity, collateral total, program fees, insurance fund, per-side open-interest accumulators, funding index, average oracle price. The pool owns the vault and is the LP mint's authority, and signs vault transfers and mint/burn CPIs with its own seeds; there is no separate signing PDA
- `custody_vault` [token account](https://solana.com/docs/terminology#token-account) PDA, seeds `["vault", pool]`: all USDC, both provider liquidity and trader collateral; `pool` is its owner
- `lp_mint` PDA, seeds `["lp_mint", pool]`: the share [mint](https://solana.com/docs/terminology#mint-account); `pool` is the mint authority

---

### Step 2: Carol provides liquidity

**Instruction:** `add_liquidity(amount = 100_000 USDC, minimum_shares_out)`

**Accounts modified:**

- `carol_usdc`: −100,000 USDC
- `custody_vault`: +100,000 USDC
- `lp_mint` → `carol_lp` (created): mints ≈100,000 shares to Carol
- `Pool.liquidity`: 0 → 100,000

The pool can now pay trader winnings, and Carol holds shares representing her slice of it.

---

### Step 3: Alice opens a 5× long

**Instruction:** `open_position(side = Long, collateral_amount = 1,000 USDC, size = 5,000 USDC, acceptable_price)`

NVDAx is at $100. The 0.1% open fee ($5) comes out of her collateral, leaving $995 of net collateral backing the position. Nothing is set aside from `Pool.liquidity` for her profit.

**Accounts modified:**

- `Position` PDA `["position", pool, alice, Long]` (created): side Long, collateral $995, size $5,000, entry price $100, entry slot (the current slot)
- `alice_usdc`: −1,000 USDC
- `custody_vault`: +1,000 USDC
- `Pool.total_collateral`: +$995
- `Pool.program_fees`: +$2.50
- `Pool.insurance_fund`: +$2.50
- `Pool` long open-interest accumulators: += this position

---

### Step 4: Bob opens a 5× short

**Instruction:** `open_position(side = Short, collateral_amount = 1,000 USDC, size = 5,000 USDC, acceptable_price)`

**Accounts modified:** a `Position` PDA `["position", pool, bob, Short]` is created; `custody_vault` +1,000 USDC; `Pool.total_collateral` +$995; `Pool.program_fees` +$2.50; `Pool.insurance_fund` +$2.50; short open-interest accumulators rise.

While both are open, **funding** accrues to the pool from the heavier side; it is settled when each position closes.

---

### Step 5: NVDA rises to $116. Alice closes in profit

**Instruction:** `close_position(minimum_payout)`

More than 10 slots have passed since she opened, so her profit has warmed up, and $116 is 16% above the pool's $100 average price, inside the 20% band, so the close goes through. Her profit is `5,000 × (116 − 100) / 100 = $800`, minus the $5 close fee. Bob's short is down the same $800, so traders are owed nothing in aggregate and the haircut `h` is one: she is paid her profit in full.

**Accounts modified:**

- `Pool.liquidity`: −$800 (providers pay her profit)
- `Pool.total_collateral`: −$995
- `Pool.program_fees`: +$2.50
- `Pool.insurance_fund`: +$2.50
- `Pool.average_price`: credits the time since the last read to $100, the price that read saw, so it stays at $100
- `Pool.last_oracle_price`: $100 → $116, which the next read credits for the time in between
- long open-interest accumulators: −= this position
- `custody_vault` → `alice_usdc`: pays out $1,790 (net collateral + profit − close fee)
- `Position` (Alice): closed; rent returned to Alice

---

### Step 6: Bob's short is underwater. Dave liquidates it

**Instruction:** `liquidate_position()`

At $116 Bob's short has lost $800; his equity ($995 − $800 = $195) has fallen below the 5% maintenance margin ($250), so anyone may close it.

**Accounts modified:**

- short open-interest accumulators: −= Bob's position
- `Pool.total_collateral`: −$995
- `Pool.liquidity`: +$800 (the loss accrues to providers)
- `custody_vault` → `dave_usdc` (created): $50 liquidation fee
- `custody_vault` → `bob_usdc`: $145 remaining equity refunded
- `Position` (Bob): closed; rent returned to Bob

Bob's equity was still positive, so the $50 fee came out of it and the insurance fund was untouched. Had the price gone far enough to take his equity below zero, Dave would have received nothing, the position would still have closed, and the insurance fund would have paid Bob's loss beyond his collateral before the providers bore any of it.

---

### Step 7: Admin collects the program's fees

**Instruction:** `collect_fees()`

**Accounts modified:** `Pool.program_fees`: $7.50 → 0; `custody_vault` pays $7.50 to `admin_usdc`. The $7.50 in `Pool.insurance_fund` stays in the vault.

---

### Step 8: Carol withdraws

**Instruction:** `remove_liquidity(shares, minimum_amount_out)`

Carol burns her shares and redeems USDC. Her balance now reflects the fees the pool earned plus the net of traders' wins and losses while she was in. A withdrawal can pay out at most `Pool.liquidity`, the tokens the providers own now; an open trader's unrealized loss counts toward her shares' value but is still in that trader's collateral.

**Accounts modified:** `lp_mint` burns Carol's shares; `Pool.liquidity` falls; `custody_vault` pays out USDC to `carol_usdc`.

---

## Design notes and further reading

The genuinely hard part of a perpetual-futures venue is keeping it solvent and permissionless *without* re-evaluating the entire market on every action. For a rigorous, Kani-checked treatment, see Anatoly Yakovenko's [percolator](https://github.com/aeyakovenko/percolator), an educational perp risk engine. It states three invariants this example also leans on, in simplified form:

- **Realizable credit**: "protected principal is senior, positive PnL is junior, and source-domain positive credit cannot exceed realizable backing reserved for that domain." Here, trader collateral is senior and trader profit is junior: `close_position` pays a winner the haircut fraction `h` of their profit, so payouts never exceed `liquidity + insurance_fund`, and a profit is paid only after the position's warm-up.
- **Account-local safety**: "every favorable action refreshes the account's full active portfolio first; … stale … legs fail closed." Here, every position and liquidity action reads a fresh oracle (stale or wide-confidence prices are rejected) and recomputes pool exposure before any payout.
- **Bounded progress**: "no public instruction needs to evaluate the whole market." Here, assets-under-management comes from running per-side accumulators, and liquidation acts on one position at a time, so no handler's cost grows with the number of open positions.

What production pool-perps (`solana-labs/perpetuals`) add that this example still leaves out: multi-asset custody with reserves in the payout token, utilization-based borrow fees, and valuing positions at the oracle's EMA rather than its spot price. This example keeps its own average only to decide when to refuse trading, and values positions at the spot price.

---

## Limitations

This is a teaching example, not an audited exchange. Notably:

- A single position per side per trader, and one collateral token per pool.
- Funding is a single time-decay index on the heavier side rather than a skew-weighted rate.

---

## Testing

The tests run in-process with [LiteSVM](https://www.anchor-lang.com/docs/testing/litesvm) and [solana-kite](https://solanakite.org); no local validator is needed. They deploy both programs, drive the mock oracle, and cover:

- liquidity round-trips, and share inflation through a provider's own trades
- opening and closing longs and shorts in profit and loss
- the initial margin on both sides of its boundary, and slippage rejection
- stale-price, pre-restart-price, and wide-confidence rejection
- funding accrual, the funding-rate maximum, an operator's wallet on the lighter side earning only the fixed rate, and funding that follows seconds rather than slots
- the price band: opens, closes, deposits and withdrawals refused when the oracle jumps outside it (`test_open_rejected_when_oracle_jumps_outside_band`, `test_close_rejected_when_oracle_jumps_outside_band`, `test_liquidity_changes_rejected_when_oracle_jumps_outside_band`), liquidation running outside it (`test_liquidation_runs_outside_band`), the exact average after each `update_price_average` (`test_single_update_moves_average_by_elapsed_fraction`), repeated updates walking the average to a genuine move until trading resumes (`test_price_average_catches_up_after_genuine_move`), and one manipulated read after an idle window leaving the average where it was (`test_one_manipulated_read_after_idle_does_not_move_average`)
- liquidation, and the refusal to liquidate a healthy position
- the haircut: a position opening without full backing (`test_open_allowed_without_full_backing`), profit paid in full while the pool backs it (`test_profit_runs_uncapped_when_backed`), two winners each paid exactly half when the pool is stressed (`test_haircut_scales_profit_when_pool_stressed`), the insurance fund paying a profit beyond `liquidity` (`test_insurance_pays_profit_beyond_liquidity`), and a winner offset by an open loser paid the pool's whole backing rather than refused (`test_winner_offset_by_open_loser_is_paid_not_refused`)
- the profit warm-up on both sides of its boundary (`test_profit_blocked_before_maturation`, `test_profit_realized_after_maturation`), and a loss closing in the slot it opened (`test_loss_not_gated_by_maturation`)
- the insurance fund: its exact share of each fee (`test_insurance_fund_funded_by_fees`), a bankrupt position's deficit paid by the fund (`test_insurance_absorbs_bankruptcy_deficit`), and a bankrupt position liquidated for no fee with the fund paying before the providers (`test_liquidation_of_bankrupt_position_charges_insurance_before_liquidity`)
- withdrawals capped at `liquidity` while traders are down (`test_remove_liquidity_capped_at_liquidity`)
- `initialize_pool`'s parameter checks, including an initial margin at or below the maintenance margin, a price band outside its range, and an insurance fee of 10,000 basis points or more
- fee collection

```bash
anchor build
cargo test --manifest-path programs/perpetual-futures/Cargo.toml
```

`anchor build` first, so the LiteSVM tests can load each program's compiled `.so` via `include_bytes!`.

## FAQ

### How do perpetual futures work on Solana?

A perp is a derivative with no expiry: traders post USDC collateral and open a leveraged long or short with `open_position`, and their profit or loss tracks an oracle price, paid in the collateral token when they `close_position`. No stock or coin ever changes hands.

### Who is the counterparty to each trade?

A shared liquidity pool. Providers fund it with `add_liquidity` and earn the trading and funding fees; the pool pays winners and keeps losers' collateral. This is the pool-collateralized design used by Jupiter Perpetuals and GMX, as opposed to the order-book design used by Drift.

### How does liquidation work?

When a position's collateral can no longer cover its loss past the maintenance margin, anyone can call `liquidate_position` to close it. Prices come from the oracle feed on every check.

### How do I run this example?

`anchor build`, then `cargo test --manifest-path programs/perpetual-futures/Cargo.toml`. The tests run against LiteSVM with a mock price feed (`initialize_feed`, `set_price`) to drive deterministic price scenarios.
