# Solana Lending (Quasar)

A Kamino/Solend-style borrow/lend program written with [Quasar](https://quasar-lang.com),
a zero-copy, `no_std` Solana framework. It is the Quasar counterpart to the Anchor
version in [`../anchor`](../anchor) and keeps the same core techniques: share-token
deposits, a kinked-curve interest index, oracle-priced obligation health, and
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
- **`PriceFeed`**: a Switchboard-On-Demand-shaped price (`mantissa * 10^exponent`
  + slot). PDA: `["price_feed", market, mint]`: scoped to a market, not to any
  individual; only the market's `owner` may write it, so prices can't be squatted
  and each market prices its own assets. `set_price` writes it directly for
  deterministic tests; in production a reserve points at the real Switchboard
  feed. Freshness is checked in slots.
- **Liquidation**: the close factor (max fraction of the debt one call repays)
  comes from the borrow reserve; the bonus from the collateral reserve. A
  repayment whose seizure would exceed the posted collateral fails with
  `LiquidationTooLarge` rather than silently seizing less, which would make the
  liquidator overpay.
- **Share tokens**: supplying mints them, redeeming burns them; the exchange rate
  `total_liquidity / share_supply` rises as borrowers pay interest.
  `available_liquidity` (not the vault's raw balance) is the source of truth, so a
  token donation can't inflate the rate.
- **Protocol fees**: the reserve keeps `reserve_factor_bps` of each interest
  accrual in `accumulated_protocol_fees` (carved out of total liquidity, so it
  never lifts the supplier exchange rate); the market owner withdraws it with
  `collect_protocol_fees`. That spread between the borrow and supply rates is how
  the owner earns.
- **Integer-only math**: `u128`, scaled by `FIXED_POINT_SCALE` (10^18), every
  conversion rounding in the protocol's favour.

### Instruction handlers (numeric discriminators)

`initialize_lending_market` (0), `initialize_reserve` (1), `set_price` (2),
`deposit_reserve_liquidity` (3), `redeem_reserve_collateral` (4),
`initialize_obligation` (5), `deposit_obligation_collateral` (6),
`withdraw_obligation_collateral` (7), `borrow_obligation_liquidity` (8),
`repay_obligation_liquidity` (9), `liquidate_obligation` (10),
`collect_protocol_fees` (11).

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
lifecycle: supply/redeem (1:1 first deposit), borrow up to the LTV limit (and
rejection beyond it), repay, interest accrual lifting the share value after slots
pass, and liquidation of an unhealthy position (with a healthy position rejected).
