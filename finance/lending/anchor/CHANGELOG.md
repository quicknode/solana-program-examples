# Changelog

## 0.1.0

Initial lending program: a Kamino/Solend-style borrow/lend market.

- Lending market, per-asset reserves with a program-owned liquidity vault and a
  share-token mint, and per-borrower obligations.
- Share-token deposit accounting with an exchange rate driven by accrued interest.
- Utilization-based kinked interest-rate curve compounded through a cumulative
  borrow-rate index; per-obligation scaled debt.
- Oracle-priced obligation health with loan-to-value and liquidation-threshold
  limits, and close-factor-capped liquidation with a seize bonus.
- Switchboard-On-Demand-shaped price feed with a `set_price` test writer.
- Rust + LiteSVM integration tests covering supply/redeem, borrow/repay,
  withdraw, interest accrual, liquidation, the share-inflation guard, and
  rounding/stale-input edge cases.
- Lending markets are isolation boundaries: every obligation handler rejects
  reserves from another market (`MarketMismatch`).
- Price feed PDAs are seeded by their authority, so no signer can write or
  pre-claim a feed another authority's reserves trust.
- Liquidation reads the close factor from the repay reserve, the bonus from the
  collateral reserve, and rejects repayments whose seizure would exceed the
  posted collateral (`LiquidationTooLarge`).
- Withdraw health checks round the removed borrow power up, so independent
  rounding can never let a withdraw pass that an exact recompute would reject.
- Reserve factor: the protocol keeps `reserve_factor_bps` of accrued interest as
  fees the market owner withdraws with `collect_protocol_fees`; the fees are
  carved out of `total_liquidity` so they never inflate the supplier exchange rate.
- LendingMarket is seeded by `(owner, market_id)` — a per-owner `u64` index —
  so one owner can run several independent markets; admin handlers authorize via
  `has_one = owner`.
