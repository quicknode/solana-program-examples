# Changelog

## 0.1.0

Initial Quasar port of the Kamino/Solend-style borrow/lend program.

- Lending market, per-asset reserves with a program-owned liquidity vault and a
  share-token mint, and isolated single-collateral / single-borrow obligations.
- Share-token deposit accounting with an exchange rate driven by accrued interest.
- Utilization-based kinked interest-rate curve compounded through a cumulative
  borrow-rate index, accrued inline per instruction.
- Oracle-priced health with loan-to-value and liquidation-threshold limits, and
  close-factor-capped liquidation with a seize bonus.
- Switchboard-On-Demand-shaped price feed with a `set_price` test writer.
- quasar-svm integration tests covering supply/redeem, borrow/repay, interest
  accrual, and liquidation (including the healthy-rejection path).
