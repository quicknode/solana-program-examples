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
- Price feed PDAs are seeded by their authority, so no signer can write or
  pre-claim a feed another authority's reserves trust.
- Liquidation reads the close factor from the borrow reserve, and rejects
  repayments whose seizure would exceed posted collateral
  (`LiquidationTooLarge`).
- Reserve factor: the protocol keeps `reserve_factor_bps` of accrued interest
  as fees the market owner withdraws with `collect_protocol_fees`.
- LendingMarket is seeded by a unique `market_id` (not the owner), so one owner
  can run several independent markets.
