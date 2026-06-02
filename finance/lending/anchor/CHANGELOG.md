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
