# Perpetual Futures Terminology

Terms used in this example, in the sense they carry here.

- **Perpetual future (perp)**: a leveraged derivative position with no expiry
  and no settlement date. Profit and loss is paid in the collateral token as the
  oracle price moves.
- **Long / short**: a long profits when the price rises, a short when it falls.
  Each is the opposite side of the pool's exposure.
- **Collateral**: the token a trader posts to back a position, and the token
  liquidity providers deposit. One pool uses one collateral token.
- **Notional size**: the position's exposure in collateral units. Profit and
  loss scales with the notional, not with the collateral posted.
- **Leverage**: notional size divided by collateral. A pool caps it through its
  initial margin: 1,000 basis points (10%) allows at most 10x.
- **Initial margin**: the net collateral, as a fraction of notional size, a
  position must post to open (`initial_margin_bps`). Always above the
  maintenance margin, so no position opens already liquidatable.
- **Equity**: a position's current worth: net collateral plus unrealized profit
  and loss, minus accrued funding. When equity falls to the maintenance margin,
  the position is liquidatable.
- **Maintenance margin**: the minimum equity, as a fraction of notional size,
  a position must keep to avoid liquidation.
- **Liquidation**: closing an under-margined position. Permissionless here: any
  caller can trigger it and earns the liquidation fee out of the position's
  remaining equity. The part of the fee the equity cannot cover is forgiven.
- **Deficit**: what a liquidated position lost beyond its collateral, when its
  equity is below zero. The insurance fund pays it first, and the liquidity
  providers bear what the fund cannot.
- **Insurance fund**: the tokens the pool holds, in `insurance_fund`, from
  `insurance_fee_bps` of every open and close fee. It pays deficits, pays a
  winner's profit once `liquidity` is exhausted, and never pays a fee.
- **Senior / junior**: a trader's collateral is senior, always theirs to
  reclaim less their losses. Their profit is junior: paid only as far as the
  pool's liquidity and insurance fund can back it.
- **Haircut ratio (`h`)**: the fraction of their profit every winner closing at
  a given moment is paid: one while `liquidity + insurance_fund` covers the
  profit owed, and that backing divided by the profit owed when it does not.
  The profit owed is the larger of traders' aggregate profit and the closing
  position's own, so a winner who closes while open losers still offset them
  is paid at most the backing, and never refused.
- **Profit warm-up**: the `profit_warmup_slots` a position must stay open before
  it can be closed at a profit. A loss is never held back.
- **Funding**: a periodic payment that anchors the pool's risk. The heavier
  side of open interest pays funding to the pool over time.
- **Open interest**: the total notional size currently open on a side.
- **Liquidity provider**: a depositor who funds the pool and is the counterparty
  to every trade, earning fees in exchange for taking the other side of trader
  profit and loss.
- **Assets-under-management**: the marked value of liquidity-provider holdings:
  pool liquidity minus the aggregate unrealized profit traders are owed.
- **Liquidity-provider share**: a token representing a pro-rata claim on
  assets-under-management.
- **Oracle feed**: the account the pool reads its price from. This example uses
  a mock oracle price feed; production points at a real one, such as
  a Pyth price feed.
- **Mark price**: the price positions are valued at. Here it is the oracle
  price directly, with no separate mark/index distinction.
- **Average price**: the pool's time-weighted moving average of the oracle
  price (`average_price`), which follows the last ten minutes of prices. Each
  oracle read credits the seconds since the previous read to the price that
  read saw (`last_oracle_price`), so a price counts only from the read that
  first sees it. Positions are never valued at it.
- **Price band**: the range around the average price, `max_price_deviation_bps`
  wide on each side, outside which the pool refuses to open or close positions
  or move liquidity. Liquidation is not refused.
