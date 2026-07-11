# Prop AMM

An oracle-quoted **proprietary AMM**: a market-making firm funds a trading
venue with its own capital and quotes both sides of it — anyone can buy the
base token at the oracle price plus a spread, or sell at the oracle price
minus it. There is no pricing curve, there are no liquidity providers, and
there are no pool shares: the operator is the only capital in the market,
which is the property that gives the design its name. This is the
architecture behind venues like Lifinity (the pioneer), SolFi, HumidiFi,
ZeroFi, Tessera, and Obric, which collectively fill most Solana swap volume
via Jupiter routing rather than their own user interfaces.

## Programs

- **`prop-amm`** — the market: one operator, one base/quote pair, one oracle
  feed, two vaults, five instruction handlers.
- **`mock-switchboard`** — a minimal stand-in for a Switchboard On-Demand
  price feed, so tests can drive deterministic price scenarios. Not for
  production.

## Key Financial Concepts

### Proprietary market making

A constant-product AMM crowdsources its capital from liquidity providers and
pays them fees; its price is a function of its reserves. A prop AMM inverts
all of it: the firm quotes prices taken from an oracle, earns the spread
instead of a fee, and risks only its own inventory. Because the price does
not depend on the pool's balances, big trades pay the same unit price as
small ones (no price impact), and there is nothing for a sandwich attacker to
squeeze — the classic front-run/back-run pattern needs a price that moves
with each trade.

### The quote: oracle, spread, bid and ask

The market pins an oracle feed at creation. Every swap reads the current
price and applies the operator's `spread_bps` each way: buyers of the base
token pay the **ask** (oracle plus spread, rounded up), sellers receive the
**bid** (oracle minus spread, rounded down). Output amounts floor. Every
rounding direction favors the market, and after the math the handler asserts
the invariant those roundings guarantee: the value leaving the vaults, at the
raw oracle price, never exceeds the value coming in.

### Inventory, not liquidity

The vault balances are not a pricing input; they only bound what the market
can deliver. A swap bigger than the inventory is rejected whole
(`InsufficientInventory`) rather than partially filled or mispriced. The
operator deposits and withdraws inventory freely — including all of it, at any
time. Nobody else has a claim on the vaults, so there is no share mint, no
pro-rata withdrawal math, and no inflation attack surface: the empty-pool
games that plague shared pools need shares to dilute, and there are none.

### Adverse selection and pulled quotes

A market maker's enemy is informed flow: traders who know the price is about
to move and hit the stale side of the quote. The two defenses this program
ships are the oracle gates (below) and `set_quote`, which lets the operator
widen the spread or pause quoting entirely. Real prop AMMs do exactly this —
during fast markets their quotes vanish and return minutes later.

### Oracle staleness and confidence

Every swap re-validates the feed: the price must be positive, at the pinned
scale, no older than 150 slots (~1 minute), and its confidence band must be
inside `max_confidence_bps`. For this design the staleness bound is not
hygiene, it is the business: a quote priced off an old number is a free
option for whoever notices first.

## Program Flow

### Participants

- **Maria** operates the market-making firm.
- **Alice** and **Bob** trade NVDAx (tokenized NVIDIA stock, 6 decimals)
  against USDC.
- The oracle quotes NVDAx at **$165** with 8 decimals of scale.

### Step 1: Maria opens the market

`initialize_market` creates the `Market` account (PDA of the mint pair), a
dataless vault-authority PDA, and the two vaults, and pins the oracle feed,
its scale, a 10 bps spread, and a 1% confidence limit. One market per pair:
the deployment is the firm.

### Step 2: Maria stocks the inventory

`deposit_inventory` moves 1,000 NVDAx and 200,000 USDC of the firm's own
tokens into the vaults. No shares are minted to anyone, because there is
nobody else to account for.

### Step 3: Alice buys 5 NVDAx at the ask

At $165 with a 10 bps spread the ask is $165.165. Alice's `swap`
(`Direction::BuyBase`) spends exactly 825.825 USDC for 5 NVDAx —
whether she bought 1 or 500, the unit price would be the same.

### Step 4: Bob sells 5 NVDAx at the bid

The bid is $164.835, so Bob's `swap` (`Direction::SellBase`) receives
exactly 824.175 USDC. A round trip through both sides costs exactly the
1.65 USDC spread — the spread is the fee, and it lands in the inventory,
not in a fee ledger.

### Step 5: The oracle reprices; the quote follows

The feed moves to $170 and the very next swap prices at $170.17 ask. No
arbitrageur had to walk the price there trade by trade, which is how a curve
AMM gets from one price to another.

### Step 6: Volatility: Maria widens the spread, then pulls quotes

`set_quote(50, false)` re-prices the market at a 50 bps spread;
`set_quote(50, true)` pauses it entirely, and swaps fail with
`MarketPaused` until she returns.

### Step 7: Maria withdraws her inventory

`withdraw_inventory` returns every token in both vaults to the firm. The
market still exists but rejects fills — an empty prop AMM refuses rather than
misprices.

## Design notes and further reading

- Production prop AMMs on Solana are closed-source and considerably more
  sophisticated: they blend multiple price sources, run inventory-skewed
  quoting (shading the quote to reduce a lopsided inventory), and integrate
  with aggregators via quote APIs. The skeleton — operator capital, oracle
  price, spread, hard oracle gates — is this program.
- Lifinity's public design notes and the Helius write-up
  "Solana's Proprietary AMM Revolution" are good next reads.
- The oracle reader deliberately reads raw bytes at fixed offsets and
  documents how to swap in `switchboard_on_demand::PullFeedAccountData::
  parse_and_verify(...)` for production.

## Limitations

- The oracle feed's owning program is not verified — the operator picks the
  feed, and a bad choice loses the operator's money, not the traders'. A
  production reader must still check the account owner.
- One flat spread both ways; no inventory skew, no size-dependent pricing.
- `paused` is the only circuit breaker; production venues also bound
  per-slot volume and single-fill size.

## Testing

```bash
anchor build
cargo test
```

The LiteSVM suite (`programs/prop-amm/tests/test_prop_amm.rs`) verifies the
quote math to the minor unit in both directions, the exact round-trip spread,
oracle repricing and re-quoting, and that every gate shuts: slippage,
staleness, confidence, pause, zero amounts, inventory bounds, and operator
access control.
