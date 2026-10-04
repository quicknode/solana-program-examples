# Solana Lending (Quasar)

A Kamino/Solend-style borrow/lend program written with [Quasar](https://quasar-lang.com),
a zero-copy, `no_std` Solana framework. It is the Quasar counterpart to the Anchor
version in [`../anchor`](../anchor) and keeps the same core techniques: share-token
deposits, a kinked-curve accumulation factor, oracle-priced obligation health, and
close-factor liquidation with a bonus.

## What's different from the Anchor version

Quasar accounts are fixed-size and zero-copy. Quasar *does* support bounded
collections (`Vec<T, N>` / `PodVec`) and remaining accounts (`CtxWithRemaining`),
the `multisig` example uses both, so a multi-asset obligation is expressible. But
the shipped Quasar DeFi examples (`escrow`, `vault`) model one position with
fixed-size accounts, so this port follows that idiom:

- **Isolated single-pair positions.** Each `Obligation` holds exactly one
  collateral reserve and one borrow reserve (fixed fields), instead of the Anchor
  version's `Vec`-based multi-asset obligation. This is the "isolated market"
  shape and removes the need for `Vec<struct>` elements and variable-account
  refreshes.
- **Inline interest accrual.** There is no separate `refresh_reserve` /
  `refresh_obligation` step: each value-dependent handler accrues the reserves it
  touches at the top of the instruction. Health is then computed inline from the
  freshly accrued reserves and the oracle prices passed in.

- **A hand-declared `LastRestartSlot` sysvar.** quasar-lang ships only the
  Clock and Rent sysvars, so `src/last_restart.rs` declares the 8-byte layout
  itself and reads it with the same `sol_get_sysvar` syscall. `price_scaled`
  uses it to reject prices published before a cluster restart, which slot-based
  staleness alone cannot catch (a halt passes hours of wall-clock time in zero
  slots).

Everything else mirrors the Anchor version.

## Major concepts

- **`LendingMarket`**: market config (owner, quote-currency mint). PDA:
  `["lending_market", market_id]`, where `market_id` is a `u64` index. Owner is
  stored as a field for authorization, not baked into the address, so one owner
  can run several isolated markets (their market 0, 1, 2 …) with no individual's
  key in a shared struct's address.
- **`Reserve`**: one asset's pool. Owns a program-controlled liquidity vault and
  a share-token mint (both PDAs, authority = the reserve), and stores the
  interest-rate config, the cumulative borrow-rate index, available liquidity, and
  scaled total debt. PDA: `["reserve", market, liquidity_mint]`.
- **`Obligation`**: a borrower's isolated position: the collateral reserve and
  deposited share amount, plus the borrow reserve and scaled debt. PDA:
  `["obligation", market, owner]`.
- **`PriceFeed`**: an oracle-shaped price (`mantissa * 10^exponent`
  + slot), with the publisher's **confidence** band in the mantissa's units
  (how far the publisher's sources disagree, half the width of the interval
  around the price). PDA: `["price_feed", market, mint]`: scoped to a market,
  not to any individual; only the market's `owner` may write it, so prices
  can't be squatted and each market prices its own assets. `set_price` takes
  the mantissa, exponent and confidence band and writes the feed directly for
  deterministic tests; in production a reserve points at a Pyth price feed and
  the program reads its `PriceUpdateV2` account instead, after checking the
  update's `feed_id`: `price_mantissa` is `price_message.price`, `exponent` is
  `price_message.exponent`, `confidence` is `price_message.conf`, and
  `last_updated_slot` is `posted_slot`. `price_scaled` makes three checks
  before any handler values anything at the price:
  - Freshness is checked in **slots** (`MAX_PRICE_STALENESS_SLOTS`), not
    wall-clock time, plus one check slots alone cannot make: a cluster restart
    passes hours of wall-clock time in zero slots, so a price stamped at or
    before the `LastRestartSlot` sysvar's slot is rejected
    (`PricePredatesRestart`), pausing valuation until the publisher posts again.
  - The price must be positive (`InvalidOraclePrice`).
  - The confidence band must be no wider than the reserve's
    `max_confidence_bps` of the price, with the comparison multiplied out as
    `confidence × 10,000 ≤ price × max_confidence_bps` in `u128` so no division
    truncates. A wider band is refused with `OracleConfidenceTooWide`: the
    oracle is reporting that it does not know the price, and a borrow,
    withdrawal or liquidation valued at a number the oracle itself doubts would
    be lending against a guess. The limit is a reserve config field beside the
    loan-to-value and liquidation thresholds, so a market owner tunes it per
    asset through `initialize_reserve`, and `validate_config` rejects a limit
    above 100% or of zero, which would refuse every live price and freeze every
    obligation holding the asset.
- **Liquidation**: the close factor (max fraction of the debt one call repays)
  comes from the borrow reserve; the bonus from the collateral reserve. A
  repayment whose seizure would exceed the posted collateral fails with
  `LiquidationTooLarge` rather than silently seizing less, which would make the
  liquidator overpay.
- **Share tokens**: supplying mints them, redeeming burns them; the exchange rate
  `total_liquidity / total_shares` rises as borrowers pay interest.
  Shares are priced from `available_liquidity`, not the vault's raw balance, so a
  token donation can't inflate the rate. That is not enough on its own, because
  total liquidity also counts interest owed on borrows and a supplier can borrow
  from their own reserve: a lone supplier can raise the value of their one share
  that way, then let rounding push it higher, until a later deposit rounds down
  in their favor. So the first deposit mints 1:1 less `MINIMUM_SHARES` (1,000),
  and `total_shares` (the share supply plus that minimum) is what every
  conversion between shares and liquidity divides by. The withheld shares
  belong to nobody, so the attacker's one share is 1 of 1,001.
- **Program fees**: the reserve keeps `reserve_factor_bps` of each interest
  accrual in `accumulated_program_fees` (carved out of total liquidity, so it
  never lifts the supplier exchange rate); the market owner withdraws it with
  `collect_program_fees`. That spread between the borrow and supply rates is how
  the owner earns.
- **Integer-only math**: `u128`, scaled by `FIXED_POINT_SCALE` (10^18), every
  conversion rounding in the program's favour, so dust cannot be extracted by
  repeated round-trips; `deposit_redeem_round_trip_creates_no_value` checks
  this by depositing and redeeming 777,777,777 units fifty times against a
  reserve whose exchange rate interest has moved off one-to-one, and asserts
  the supplier never holds more than they started with.
- **Interest on the wall clock**: a reserve's rate curve is annual, and the
  conversion to a per-second rate divides by `SECONDS_PER_YEAR`. Elapsed time is
  the Clock's `unix_timestamp` minus the reserve's `last_accrual_timestamp`, so a
  borrower pays the advertised APR over a real year whatever the cluster's slot
  time is. Counting slots instead would need a slots-per-year divisor, which is a
  guess at the slot length: the network changes that length by feature gate, and
  does not deliver it exactly between changes. The timestamp is written by each
  block's leader, but the runtime bounds how far one block can move it, so an
  elapsed time is out by a second or two at most, which is nothing against an
  annual rate. A timestamp at or before the stored one accrues nothing.

### Instruction handlers (numeric discriminators)

`initialize_lending_market` (0), `initialize_reserve` (1), `set_price` (2),
`deposit_reserve_liquidity` (3), `redeem_reserve_collateral` (4),
`initialize_obligation` (5), `deposit_obligation_collateral` (6),
`withdraw_obligation_collateral` (7), `borrow_obligation_liquidity` (8),
`repay_obligation_liquidity` (9), `liquidate_obligation` (10),
`collect_program_fees` (11).

## Setup

- Rust and the Solana toolchain (`cargo-build-sbf`).
- Quasar (`quasar-lang` / `quasar-spl`), pinned to the rev used across the repo's
  Quasar examples (see `Cargo.toml` for the rationale).

## Testing

```sh
cargo build-sbf          # produces target/deploy/quasar_lending.so
cargo test tests::       # runs the quasar-svm integration tests
```

`cargo build-sbf` must run first: the tests load the compiled
`target/deploy/quasar_lending.so` into `quasar-svm`. The suite drives the full
lifecycle: supply/redeem (1:1 first deposit, less the withheld minimum), borrow up to the LTV limit (and
rejection beyond it), a price whose confidence band is too wide, repay, interest
accrual lifting the share value after time passes, interest that follows seconds
rather than slots (and charges nothing for a timestamp behind the last accrual),
liquidation of an unhealthy position (with a healthy position rejected), and
rounding edges.
