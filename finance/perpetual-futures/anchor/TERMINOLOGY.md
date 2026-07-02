# Perpetual Futures Terminology

Terms used in this example, in the sense they carry here.

- **Perpetual future (perp)** — a leveraged derivative position with no expiry
  and no settlement date. Profit and loss is paid in the collateral token as the
  oracle price moves.
- **Long / short** — a long profits when the price rises, a short when it falls.
  Each is the opposite side of the pool's exposure.
- **Collateral** — the token a trader posts to back a position, and the token
  liquidity providers deposit. One pool uses one collateral token.
- **Notional size** — the position's exposure in collateral units. Profit and
  loss scales with the notional, not with the collateral posted.
- **Leverage** — notional size divided by collateral. A pool caps it at
  `max_leverage`.
- **Equity** — a position's current worth: net collateral plus unrealized profit
  and loss, minus accrued funding. When equity falls to the maintenance margin,
  the position is liquidatable.
- **Maintenance margin** — the minimum equity, as a fraction of notional size,
  a position must keep to avoid liquidation.
- **Liquidation** — closing an under-margined position. Permissionless here: any
  caller can trigger it and earns the liquidation fee.
- **Senior / junior claim** — a trader's deposited collateral is *senior*: always
  reclaimable in full. Their profit is *junior*: only as real as the backing
  behind it, and scaled down by the haircut when the pool is stressed.
- **Haircut ratio (`h`)** — a single global number, between zero and one, that
  every closing winner's profit is multiplied by. One when the pool can back all
  profit; below one when it cannot, the same fraction for everyone — no queue, no
  singled-out trader.
- **Profit maturation (warm-up)** — profit cannot be realized until a position has
  been open `profit_warmup_slots` slots. An oracle spike's paper gain cannot be
  cashed out before the manipulation passes. Loss is never gated this way.
- **Insurance fund** — a senior buffer, funded by a cut of fees, that absorbs a
  bankrupt position's deficit before liquidity providers do, and counts as
  backing for trader profit in the haircut.
- **Funding** — a periodic payment that anchors the pool's risk. The heavier
  side of open interest pays funding to the pool over time.
- **Open interest** — the total notional size currently open on a side.
- **Liquidity provider** — a depositor who funds the pool and is the counterparty
  to every trade, earning fees in exchange for taking the other side of trader
  profit and loss.
- **Assets-under-management** — the marked value of liquidity-provider holdings:
  pool liquidity minus the aggregate unrealized profit traders are owed.
- **Liquidity-provider share** — a token representing a pro-rata claim on
  assets-under-management.
- **Oracle feed** — the account the pool reads its price from. This example uses
  a mock Switchboard On-Demand feed; production points at a real one.
- **Mark price** — the price positions are valued at. Here it is the oracle
  price directly, with no separate mark/index distinction.
