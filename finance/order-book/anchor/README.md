# Order Book - Central Limit Order Book (CLOB)

This is an **[order book](https://www.investopedia.com/terms/o/order-book.asp)** - specifically, a **[central limit order
book (CLOB)](https://www.investopedia.com/terms/o/order-book.asp)**, the standard piece of market infrastructure used by
NYSE, NASDAQ, LSE, CME, and every major crypto venue. An Anchor
program that runs an onchain order book for a single pair of token mints:
users post buy or sell offers at the prices they want, the program
matches crossing offers in price-time priority, and settles the
resulting token movements.

If you already know what an order book, a limit order, and a taker fee
are, skip to [Accounts and PDAs](#2-accounts-and-pdas) or
[Instruction lifecycle walkthrough](#3-instruction-lifecycle-walkthrough).

---

## Table of contents

1. [What does this program do?](#1-what-does-this-program-do)
- [A real-world walkthrough: NVDAx/USDC](#a-real-world-walkthrough-nvdaxusdc)
2. [Accounts and PDAs](#2-accounts-and-pdas)
3. [Instruction lifecycle walkthrough](#3-instruction-lifecycle-walkthrough)
4. [The matching engine - step by step](#4-the-matching-engine--step-by-step)
- [Ensuring fast order matching performance](#ensuring-fast-order-matching-performance)
5. [Full-lifecycle worked examples](#5-full-lifecycle-worked-examples)
6. [Safety and edge cases](#6-safety-and-edge-cases)
7. [Running the tests](#7-running-the-tests)
8. [Extending the program](#8-extending-the-program)

---

## 1. What does this program do?

Two users want to swap tokens at prices they each picked:

- Alice holds **USDC** (the *[quote](https://www.investopedia.com/terms/q/quotecurrency.asp)* mint - the pricing unit, the way USD
  is the pricing unit in "NVDAx is $950") and wants to buy **NVDAx**
  (the *[base](https://www.investopedia.com/terms/b/basecurrency.asp)* mint - the asset being priced), but only if she can
  get NVDAx at 900 USDC per share or lower.
- Bob holds **NVDAx** and wants USDC, but only if he can get at least
  950 USDC per NVDAx share he sells.

They post their offers - Alice a *bid* (a buy offer at a limit price),
Bob an *ask* (a sell offer at a limit price) - and wait. Alice's bid
sits on the book. Bob's ask sits on the book. Neither crosses the
other, so nothing happens yet.

Later, Carol shows up holding NVDAx and willing to sell at any price ≥ 900
USDC. She posts an ask at 900. Now Alice's bid (900 USDC) *crosses*
Carol's new ask (900 USDC) - the bid is ≥ the ask. The program:

1. Pairs them up.
2. Locks Carol's NVDAx in the program's base vault (Carol signed this
   transaction, so only her funds move).
3. Allocates Alice's USDC - already sitting in the quote vault since
   Alice placed her bid - to Carol.
4. Credits each party's unsettled balance with what they're owed, minus
   a fee for the market operator. Tokens don't leave the vaults yet;
   Alice and Carol each call `settle_funds` later to pull them out.

At no point does either of them transfer directly to the other - all
token flows go through two program-owned vaults, and both users later
call `settle_funds` to pull their balances out.

### The onchain pieces, in plain terms

- A **Market** PDA - one per base/quote pair. Stores fee rate, tick
  size, minimum order size, the addresses of the four related accounts
  (base vault, quote vault, fee vault, order book), and the pubkey
  that can withdraw accumulated fees.
- An **OrderBook** account - two stores: bids sorted highest-first,
  asks sorted lowest-first, each holding up to 1024 entries. Rather
  than a plain list of orders, each side uses a depth-bounded tree (a
  critbit trie) for fast lookup - see [Ensuring fast order matching performance](#ensuring-fast-order-matching-performance).
  Each entry stores enough to drive matching (price, quantity,
  `order_id`); the full `Order` PDA holds the authoritative state.
- A **MarketUser** PDA - one per `(market, wallet)` pair. Tracks the
  order_ids this user has open and two running tallies
  (`unsettled_base`, `unsettled_quote`) of tokens owed back to this
  user from fills or cancellations.
- An **Order** PDA - one per placed order. Stores price, quantity,
  side (bid or ask), fill status, and the owner.
- Three token accounts held by the Market PDA: `base_vault` (all
  sellers' locked base + buyers' bought base waiting to be withdrawn),
  `quote_vault` (mirror for quote), and `fee_vault` (accumulated taker
  fees).

### Finance background, briefly

For readers new to trading terms - these are the same concepts every
equity, futures, and crypto exchange uses. They're optional;
everything above describes the program mechanically.

- **A [limit order](https://www.investopedia.com/terms/l/limitorder.asp)** is an order to trade an amount of an asset at a
  specific price *or better*. A *[bid](https://www.investopedia.com/terms/b/bid.asp)* is a limit order to buy, an
  *[ask](https://www.investopedia.com/terms/a/ask.asp)* is a limit order to sell. The "limit" part means: don't trade
  at a worse price than the one I named.

- **An order book** is the set of currently-open bids and asks,
  sorted so the best price on each side sits at the top. The "top of
  book" on the bid side is the highest-priced buy offer; the top of
  book on the ask side is the lowest-priced sell offer.

- **A [maker](https://www.investopedia.com/terms/m/marketmaker.asp)** is whoever posts an order that doesn't immediately
  match - they "make" [liquidity](https://www.investopedia.com/terms/l/liquidity.asp) by leaving their offer on the book
  for others to trade against. A **[taker](https://www.investopedia.com/terms/m/maker_taker.asp)** is whoever walks into the
  book and hits the resting orders - they "take" liquidity.

- **A [taker fee](https://www.investopedia.com/terms/m/maker_taker.asp)** is a cut of each trade taken by the venue from the
  taker's leg of the trade, expressed in *[basis points](https://www.investopedia.com/terms/b/basispoint.asp)* (bps). One
  bps is 0.01%; 10 000 bps is 100%. A 50 bps fee is 0.5%.

- **Price-time priority** is the universal matching rule on every
  limit order book: best price first, and at the same price level,
  whoever posted first fills first.

- **[Settlement](https://www.investopedia.com/terms/s/settlement.asp)** is the step that actually moves tokens out of the
  venue's custody account and back to the user. This program splits
  matching and settlement into two instruction handlers (`place_order`
  and `settle_funds`) so a taker crossing a long list of makers
  doesn't have to pay for a token CPI per maker.

### What this example is not

- **Not deployed, not audited.** Treat as a learning example, not
  production-ready code.
- **No [immediate-or-cancel](https://www.investopedia.com/terms/i/immediateorcancel.asp) (IOC), [fill-or-kill](https://www.investopedia.com/terms/f/fill-or-kill.asp) (FOK), or post-only orders** - every
  order matches what it can at the limit price and rests any remainder
  on the book. IOC would discard the remainder instead of resting it;
  FOK would reject the whole order unless it fills entirely; post-only
  would reject the order if it would cross immediately.
- **No circuit breakers, no oracles, no price bands.**

Solana terminology (account, PDA, CPI, bump, discriminator, signer,
lamport, ATA) is defined at <https://solana.com/docs/references/terminology>.

**Base asset, quote asset.** In "BASE/QUOTE", the base is the asset
being priced and the quote is the pricing unit. Bids spend quote and
receive base; asks spend base and receive quote.

**Limit price.** The worst price at which an order is allowed to
trade - for a bid, the *highest* the buyer will pay; for an ask, the
*lowest* the seller will accept. A bid at 900 won't fill against an
ask at 950.

**Tick size.** Smallest allowable price increment. A market with
`tick_size = 10` accepts prices 10, 20, 30, …, but rejects 15. Stops
the book filling up with 1-unit-apart orders.

**Minimum order size.** Smallest allowable `quantity` on any order.
Keeps dust orders from polluting the book.

**Match / fill / cross.** Two orders *cross* when the bid's price is
≥ the ask's price; they *match* (are paired up) and a *fill* is the
result - one crossing event with a fill quantity and a fill price.
One call to `place_order` can produce many fills.

**[Price improvement](https://www.investopedia.com/terms/p/priceimprovement.asp).** When a taker's limit is better than the best
resting price on the opposite side, the fill happens at the resting
(maker's) price. The taker gets a better deal than they named; the
difference is refunded to the taker's `unsettled_quote`.

**Unsettled balance.** Two `u64` counters on each `MarketUser`:
`unsettled_base` and `unsettled_quote`. Fills, price-improvement
rebates, and cancellations all increase these counters. The physical
tokens still sit in the market's vaults. `settle_funds` moves them
to the user's own token accounts and zeroes the counters.

**Fee vault.** A separate token account (quote mint) owned by the
Market PDA. Every taker fee - `ceil(gross * fee_bps / 10_000)` per fill -
moves here in one batched CPI at the end of `place_order`.

**Remaining accounts.** Solana lets the caller pass a tail of extra
`AccountInfo`s beyond the ones named in `#[derive(Accounts)]`. The
`place_order` handler uses them for the resting orders the taker
wants to cross: for each one, the caller supplies
`(maker_order_pda, maker_user_account_pda)` in the book's price-time
order.

---

## A real-world walkthrough: NVDAx/USDC

This section walks through a complete sequence of trades using four real participants and two example tokens. Every instruction handler is shown with the exact accounts it creates or mutates. Read this before the technical reference sections if you are new to order books or to Solana.

### The tokens

| Token | What it is | Role on this market |
|---|---|---|
| **NVDAx** | An onchain NVIDIA share (xStock). Its price tracks the underlying stock. | **Base asset** - the thing being bought and sold |
| **USDC** | A stablecoin redeemable 1:1 for US dollars | **Quote asset** - the currency used for pricing and payment |

A price of **960** means "960 USDC per NVDAx". The same program logic - identical instruction handlers and account structure - works for any other pair, such as **TSLAx/USDC** (Tesla xStock).

### The participants

| Name | Role | Motivation |
|---|---|---|
| **Maria** | Market authority | Earns 0.25 % ([25 basis points](https://www.investopedia.com/terms/b/basispoint.asp)) on every fill. Her revenue scales with market volume, so she wants a liquid, trusted venue. |
| **Alice** | Retail investor - buyer | Bullish thesis: she expects NVDAx to rise from ~960 USDC to ~1 100 as demand for NVIDIA's AI chips grows. She wants to accumulate NVDAx at a good price before that move. |
| **Bob** | [Market maker](https://www.investopedia.com/terms/m/marketmaker.asp) | No directional view on NVDAx. Profits from the [bid-ask spread](https://www.investopedia.com/terms/b/bid-askspread.asp): he simultaneously quotes a buy price (bid) below fair value and a sell price (ask) above it. If both sides fill, the difference is his gross revenue. He provides [liquidity](https://www.investopedia.com/terms/l/liquidity.asp) to the market in exchange for that spread. |
| **Carol** | Retail investor - seller | Bought NVDAx at 800 USDC six months ago. It is now trading around 960. She wants to sell some to [realise her profit](https://www.investopedia.com/terms/r/realizedprofit.asp) in USDC. |

---

### Step 1 - Maria creates the market

**Instruction: `initialize_market(fee_basis_points=25, tick_size=1, min_order_size=1)`**
**Key accounts: `base_mint = NVDAx`, `quote_mint = USDC`**

Maria's wallet signs. Five accounts are created:

| Account | Type | Seeds / notes | State after |
|---|---|---|---|
| `Market` PDA | Program data | `["market", NVDAx_mint, USDC_mint]` | `fee_bps=25`, `tick_size=1`, `is_active=true`; vault addresses recorded |
| `OrderBook` | Zero-copy slab (~180 KB) | Client-allocated (not a PDA) | Both critbit trees empty |
| `base_vault` | Token account (NVDAx) | Authority = Market PDA | 0 NVDAx |
| `quote_vault` | Token account (USDC) | Authority = Market PDA | 0 USDC |
| `fee_vault` | Token account (USDC) | Authority = Market PDA | 0 USDC |

**No tokens move.** Maria pays the SOL rent for all five accounts.

---

### Step 2 - Alice, Bob, and Carol register as traders

**Instruction: `create_market_user`** (called once by each trader)

Each call creates one `MarketUser` PDA - a per-(trader, market) account that tracks their open orders and any tokens owed to them:

| Account | Seeds | State after |
|---|---|---|
| Alice's `MarketUser` PDA | `["market_user", market, alice_pubkey]` | `unsettled_base=0`, `unsettled_quote=0`, `open_orders=[]` |
| Bob's `MarketUser` PDA | `["market_user", market, bob_pubkey]` | same |
| Carol's `MarketUser` PDA | `["market_user", market, carol_pubkey]` | same |

---

### Step 3 - Bob posts a sell offer (ask) at 965 USDC

Bob estimates NVDAx fair value at 960 USDC. He quotes a 10-USDC spread - ask at 965, bid at 955. He starts by posting the ask.

**Instruction: `place_order(side=Ask, price=965, quantity=10)`** (no `remaining_accounts` - book is empty)

**Token flow:**
```
bob_nvdax_ata --[10 NVDAx]--> base_vault
```

**Accounts changed:**

| Account | Change |
|---|---|
| `base_vault` | +10 NVDAx |
| New `Order` PDA (id=1) | `side=Ask, price=965, qty=10, status=Open` |
| `OrderBook.asks` | Leaf inserted at price 965 |
| Bob's `MarketUser.open_orders` | `[1]` |

**Book state:**
```
asks  [(id=1, price=965, qty=10)]   ← Bob
bids  []
```

---

### Step 4 - Alice places a buy offer (bid) at 950 USDC

Alice places a [limit order](https://www.investopedia.com/terms/l/limitorder.asp): she will buy 5 NVDAx but pay no more than 950 USDC each. Her bid (950) does not cross Bob's ask (965), so nothing fills and her bid rests on the book.

**Instruction: `place_order(side=Bid, price=950, quantity=5)`** (no `remaining_accounts`)

**Token flow:**
```
alice_usdc_ata --[950 × 5 = 4 750 USDC]--> quote_vault
```

**Accounts changed:**

| Account | Change |
|---|---|
| `quote_vault` | +4 750 USDC |
| New `Order` PDA (id=2) | `side=Bid, price=950, qty=5, status=Open` |
| `OrderBook.bids` | Leaf inserted at price 950 |
| Alice's `MarketUser.open_orders` | `[2]` |

**Book state:**
```
asks  [(id=1, price=965, qty=10)]   ← Bob
bids  [(id=2, price=950, qty=5)]    ← Alice
```

The [bid-ask spread](https://www.investopedia.com/terms/b/bid-askspread.asp) is 965 − 950 = 15 USDC. No trade yet.

---

### Step 5 - Carol sells into Alice's bid

Carol wants to sell 3 NVDAx. Alice is bidding 950 USDC - above Carol's floor of 945. Carol sends an [ask](https://www.investopedia.com/terms/a/ask.asp) at 945 and passes Alice's resting order as a maker.

**Instruction: `place_order(side=Ask, price=945, quantity=3, remaining_accounts=[alice_order_pda, alice_market_user_pda])`**

**Crossing check:** Carol's ask (945) ≤ Alice's bid (950) ✓ - the orders cross. Fill price = 950 (Alice's price - the resting [maker](https://www.investopedia.com/terms/m/marketmaker.asp) always sets the execution price). Carol named 945 but receives 950 - that is [price improvement](https://www.investopedia.com/terms/p/priceimprovement.asp).

**Token flow (Carol's NVDAx locked up front):**
```
carol_nvdax_ata --[3 NVDAx]--> base_vault
```

**Fill accounting (fill price = 950, fill qty = 3):**

| Line item | Calculation | Result |
|---|---|---|
| Gross quote exchanged | 950 × 3 | 2 850 USDC |
| Taker fee (25 bps) | ceil(2 850 × 25 / 10 000) = ceil(7.125) | 8 USDC |
| Carol's net proceeds | 2 850 − 8 | 2 842 USDC → `carol.MarketUser.unsettled_quote` |
| Alice's base received | 3 NVDAx | → `alice.MarketUser.unsettled_base` |

**Accounts changed:**

| Account | Change |
|---|---|
| `base_vault` | +3 NVDAx (Carol's lock) |
| `fee_vault` | +8 USDC (fee CPI from quote_vault) |
| Alice's `Order` PDA (id=2) | `filled_quantity=3`, `status=PartiallyFilled` |
| Alice's `MarketUser.unsettled_base` | +3 NVDAx |
| Alice's `MarketUser.open_orders` | `[2]` (still open - 2 of 5 NVDAx remain) |
| Carol's `MarketUser.unsettled_quote` | +2 842 USDC |
| New Carol's `Order` PDA (id=3) | `side=Ask, price=945, qty=3, status=Filled` |
| `OrderBook.bids` | Alice's leaf quantity: 5 → 2 |

**Book state:**
```
asks  [(id=1, price=965, qty=10)]   ← Bob (untouched)
bids  [(id=2, price=950, qty=2)]    ← Alice (3 filled, 2 still resting)
```

Alice has 3 NVDAx credited to her (tracked in `unsettled_base`). Carol has 2 993 USDC credited (tracked in `unsettled_quote`). Neither amount has left the vaults yet - that happens on `settle_funds`.

---

### Step 6 - Settlement: tokens move to wallets

[Settlement](https://www.investopedia.com/terms/s/settlement.asp) is when the program pays out what it owes.

**Alice calls `settle_funds`:**
```
base_vault --[3 NVDAx]--> alice_nvdax_ata
```
`alice.MarketUser.unsettled_base = 0`

**Carol calls `settle_funds`:**
```
quote_vault --[2 842 USDC]--> carol_usdc_ata
```
`carol.MarketUser.unsettled_quote = 0`

---

### Step 7 - Maria sweeps fees

**Maria calls `withdraw_fees`:**
```
fee_vault --[8 USDC]--> maria_usdc_ata
```
`fee_vault.balance = 0`

---

### Final position

| Participant | Paid / locked | Received | Outcome |
|---|---|---|---|
| **Alice** | 4 750 USDC (for 5 NVDAx) | 3 NVDAx + 1 900 USDC still in `quote_vault` (2-NVDAx bid resting at 950) | Thesis running; waiting for a seller at 950 to fill the rest |
| **Carol** | 3 NVDAx (cost 800 each) | 2 842 USDC | Locked in ≈ 147 USDC/NVDAx profit net of fee |
| **Bob** | 10 NVDAx locked | Nothing yet - ask at 965 unfilled | Earns the spread when a buyer at 965 arrives |
| **Maria** | - | 8 USDC | Fee revenue |

Alice's remaining 2-NVDAx [bid](https://www.investopedia.com/terms/b/bid.asp) stays on the book. The next seller willing to part with NVDAx at 950 or below will fill it automatically. A **TSLAx/USDC** market runs the same seven steps with different mint addresses.

---

## 2. Accounts and PDAs

### State / data accounts

| Account | PDA? | Seeds | Authority | Holds |
|---|---|---|---|---|
| `Market` | yes | `["market", base_mint, quote_mint]` | program | fee rate, tick size, min order size, base/quote mint pubkeys, vault pubkeys, order book pubkey, `authority` wallet (allowed to withdraw fees) |
| `OrderBook` | no (client-allocated keypair) | n/a — too large (~180 KB) for an `init`/CPI PDA, so created via `create_account` (which needs a signing key a PDA lacks); tied to its market via `has_one` | program | two critbit trees (bids highest-first, asks lowest-first, 1024 leaves each), `next_order_id` |
| `Order` | yes | `["order", market, order_id.to_le_bytes()]` | program | owner, side, price, original_quantity, filled_quantity, status, timestamp |
| `MarketUser` | yes | `["market_user", market, owner]` | program | `unsettled_base`, `unsettled_quote`, `open_orders: Vec<u64>` (max 20) |

### Token accounts (owned by the Token Program, authority = Market PDA)

| Account | PDA? | Authority | Mint | Holds |
|---|---|---|---|---|
| `base_vault` | no (regular token account) | Market PDA | base | bids' locked base IS NOT STORED HERE - only asks' locked base sits here pre-match, plus base owed to bid-takers waiting for `settle_funds` |
| `quote_vault` | no | Market PDA | quote | bids' locked quote pre-match, plus quote owed to ask-takers and bid-makers waiting for settlement |
| `fee_vault` | no | Market PDA | quote | taker fees accumulated across all fills; drained by `withdraw_fees` |

Note: the **token vaults are not PDAs**. They are regular token
accounts created with `init` in `initialize_market.rs`; their
*authority* is the Market PDA, so only the program can move funds out.
Their addresses are computed by the caller (e.g. generated Keypairs in
the tests) and then written to `market.base_vault` / `quote_vault` /
`fee_vault` for the program to validate them on later calls via
`has_one = fee_vault` etc.

### Leaf layout in the `OrderBook` slab

Each side of the book is a critbit tree whose leaves are 88-byte
`LeafNode`s:

```rust
pub struct LeafNode {
    pub key: u128,          // high 64 bits = price; low 64 = seq_num (time priority)
    pub owner: Pubkey,
    pub quantity: u64,      // remaining quantity on this resting order
    pub order_id: u64,      // links to the full Order PDA
    pub timestamp: i64,
}
```

The full order state (`filled_quantity`, `status`, `original_quantity`)
lives on the `Order` PDA. The leaf holds just enough to drive matching
(price via the key, remaining quantity) and to let offchain tooling
display age and link back to the order.

### `Order` state

From [`state/order.rs`](programs/order-book/src/state/order.rs):

```rust
pub struct Order {
    pub market: Pubkey,
    pub owner: Pubkey,
    pub order_id: u64,
    pub side: OrderSide,           // Bid | Ask
    pub price: u64,
    pub original_quantity: u64,
    pub filled_quantity: u64,
    pub status: OrderStatus,       // Open | PartiallyFilled | Filled | Cancelled
    pub timestamp: i64,
    pub bump: u8,
}
```

`remaining_quantity(order) = original_quantity - filled_quantity`. Used
by `cancel_order` to decide how much to credit back to the user.

### `MarketUser` state

```rust
pub struct MarketUser {
    pub market: Pubkey,
    pub owner: Pubkey,
    pub unsettled_base: u64,
    pub unsettled_quote: u64,
    pub open_orders: Vec<u64>,   // capped at 20 via Anchor max_len
    pub bump: u8,
}
```

The `open_orders` cap (20 per user) is mirrored by a
`MAX_OPEN_ORDERS_PER_USER` check in `place_order`. One user cannot
flood the book.

**Why per-(user, market) and not per-user?** A `MarketUser` is keyed
by both the human `owner` and the `market`, not by `owner` alone.
Three reasons:

1. **Unsettled balances are per-market by definition.** Different
   markets use different `base_mint` / `quote_mint` pairs, so the
   scalar `unsettled_base` / `unsettled_quote` fields can't be
   shared across markets - they'd refer to different tokens.

2. **Open-order indexing is local to one book.** `open_orders`
   holds `order_id`s that index into a specific market's
   `OrderBook`. Mixing ids from different books would force a
   per-entry market discriminator and a wider lookup path.

3. **Lock-contention isolation.** A user trading on multiple
   markets in parallel would otherwise serialise every
   `place_order` / `settle_funds` on a single shared account.
   Per-(user, market) lets independent markets run independently.

This matches the standard pattern: Openbook v2 calls it
`OpenOrdersAccount`, Phoenix calls it `Trader`, Serum called it
`OpenOrders`. We named ours `MarketUser` to be explicit about what
it actually scopes to.

### How vault balances evolve

At any point in time:

- `base_vault.balance` = sum of all resting asks' `remaining_quantity`
  + every user's `unsettled_base`.
- `quote_vault.balance` = sum of all resting bids'
  `price * remaining_quantity`
  + every user's `unsettled_quote`.

(Plus the bit of quote that the matching engine has already taken out
as fee and batched into `fee_vault`.)

This is not a hard invariant the program enforces - it emerges from
the flows. The invariant worth caring about is the per-event balance:
every fill moves tokens from the loser's locked pool to the winner's
`unsettled_*`, plus the fee cut to `fee_vault`. The unit tests check
this directly (`settle_funds_after_match_pays_out_both_unsettled_balances`).

---

## 3. Instruction lifecycle walkthrough

The program has six instruction handlers. The order a user encounters
them is:

1. `initialize_market` (market operator - once)
2. `create_market_user` (every user, once per market)
3. `place_order` (a user - as many times as they want)
4. `cancel_order` (a user - to remove a resting order)
5. `settle_funds` (a user - to collect winnings)
6. `withdraw_fees` (market authority - to collect protocol revenue)

For each, the shape is: who signs, what accounts go in, what PDAs get
created, what token flows happen, what state mutates, what checks are
run.

Token flow shorthand:

```
  <source> --[amount of <mint>]--> <destination>
```

### 3.1 `initialize_market`

**Who calls it:** the market operator. They create a new trading pair.

**Signers:** `authority`.

**Parameters:**

```rust
pub fn initialize_market(
    context: Context<InitializeMarket>,
    fee_basis_points: u16,
    tick_size: u64,
    min_order_size: u64,
) -> Result<()>
```

**Accounts in:**

- `authority` (signer, mut - pays account rent for all five new
  accounts)
- `market` (PDA, **init**, seeds `["market", base_mint, quote_mint]`)
- `order_book` (not a PDA - client calls `system_program::create_account`
  first, sized to `ORDER_BOOK_ACCOUNT_SIZE`; verified here with
  `#[account(zero)]`)
- `base_mint`, `quote_mint` (read-only)
- `base_vault`, `quote_vault`, `fee_vault` (all **init** as
  `TokenAccount`s, authority = `market`)
- `token_program`, `system_program`

**Checks:**

- `tick_size > 0` → `InvalidTickSize`
- `min_order_size > 0` → `BelowMinOrderSize`
- `fee_basis_points <= 10_000` → `InvalidFeeBasisPoints`

**Token movements:** none (the vaults are empty after init).

**State changes:** `market` and `order_book` accounts are written with
the supplied parameters plus all the derived fields
(`market.authority`, the vault pubkeys, `is_active = true`,
`next_order_id = 1`).

The vaults are regular token accounts, *not* PDAs - their
addresses are chosen by the caller (typically fresh keypairs) and
captured on the market's state so later instruction handlers can
validate them.

### 3.2 `create_market_user`

**Who calls it:** every user, exactly once per market they want to
trade on.

**Signers:** `owner`.

**Accounts in:**

- `owner` (signer, mut - pays rent)
- `market` (read-only)
- `market_user` (PDA, **init**, seeds `["market_user", market, owner]`)
- `system_program`

**Token movements:** none.

**State changes:** new `MarketUser` with all counters zero and no
open orders.

### 3.3 `place_order`

**Who calls it:** anyone with a `MarketUser` for this market.

**Signers:** `owner`.

**Parameters:**

```rust
pub fn place_order<'info>(
    context: Context<'info, PlaceOrder<'info>>,
    side: OrderSide,   // Bid | Ask
    price: u64,
    quantity: u64,
) -> Result<()>
```

**Accounts in (named):**

- `market` (mut, `has_one = fee_vault`)
- `order_book` (mut, PDA seeds-checked)
- `order` (PDA, **init**, seeds
  `["order", market, next_order_id.to_le_bytes()]`)
- `market_user` (mut, PDA seeds-checked)
- `base_vault`, `quote_vault`, `fee_vault` (all mut, boxed)
- `user_base_account`, `user_quote_account` (mut - the caller's ATAs)
- `base_mint`, `quote_mint` (read-only)
- `owner` (signer, mut)
- `token_program`, `system_program`

**Accounts in (remaining):** a list of `AccountInfo`s passed via the
transaction's remaining accounts, grouped in pairs. For each resting
order the caller wants the taker to cross, in the book's price-time
order:

```
remaining_accounts[2*i]     = maker_order_pda (Order account)
remaining_accounts[2*i + 1] = maker_user_account_pda (MarketUser)
```

If the caller doesn't pass any pairs, the order is treated as
pure-maker: whatever part of it is allowed by the book state becomes a
resting order.

**Checks (top of handler):**

- `market.is_active` → `MarketPaused`
- `price > 0` → `InvalidPrice`
- `price % tick_size == 0` → `InvalidTickSize`
- `quantity >= min_order_size` → `BelowMinOrderSize`
- `open_orders.len() < 20` (mirror of the max_len on the struct) →
  `TooManyOpenOrders`
- `remaining_accounts.len() % 2 == 0` → `MissingMakerAccounts`

**Checks (per maker pair, during planning):**

- Maker order's `order_id` exists in the relevant book side →
  `MakerAccountMismatch`
- Maker order's `market == market.key()` → `MakerAccountMismatch`
- Maker pair index == the maker's slot position on the book
  (i.e. caller walked the book sorted by price) → `MakerAccountMismatch`

**Checks (per fill, during execution):**

- Maker order and user account have matching `owner` →
  `MakerOwnerMismatch`
- Maker user account's `market == market.key()` →
  `MakerAccountMismatch`

**Checks (before resting remainder):**

- the taker's side of the book isn't at its 1024-leaf capacity →
  `OrderBookFull`
- Integer math throughout: every multiplication uses
  `checked_mul`; every addition on balances uses `checked_add`;
  every product of two `u64` money values is computed in `u128`
  to avoid intermediate overflow and then narrowed back to `u64`
  with `try_into` → `NumericalOverflow`. After each per-fill fee
  calculation an invariant check enforces `fee_quote <= gross_quote`.

**Token movements (up front):**

For a **bid**:
```
  user_quote_account --[price * quantity of quote_mint]--> quote_vault
```

For an **ask**:
```
  user_base_account --[quantity of base_mint]--> base_vault
```

The full lock happens regardless of whether the order will fully fill
immediately. That keeps the vault invariant simple: the token account
always holds *exactly* what's needed to fulfil every open trading
position plus every unsettled balance.

**Token movements (during matching, per fill):** see
[§4. The matching engine - step by step](#4-the-matching-engine--step-by-step).
Summary:

- For a taker bid crossing a resting ask at price `p`:
  ```
  quote_vault         --[p * fill_qty * fee_bps / 10_000]--> fee_vault
  (everything else stays in quote_vault as unsettled_quote for maker)
  (base_vault provides the taker's base via unsettled_base - the base
   was pre-locked when the maker placed their ask)
  ```

- For a taker ask crossing a resting bid at price `p`:
  ```
  quote_vault         --[p * fill_qty * fee_bps / 10_000]--> fee_vault
  ```

No user's ATA is touched during matching - all movements happen
between vaults or inside `MarketUser` counters. Physical payouts wait
for `settle_funds`.

**PDAs created:** `order` (always; even fully-crossed takers get an
Order PDA, marked `Filled` immediately, for consistency with
indexers).

**State changes:**

On the taker's `MarketUser`:

- `unsettled_base += sum of fill.fill_quantity` (taker bid side)
- `unsettled_quote += sum of price_improvement_rebate`
  (taker bid side, per fill)
- `unsettled_quote += sum of (gross - fee)` (taker ask side)

On each maker's `Order` (via `Account::try_from` + `exit`):

- `filled_quantity += fill.fill_quantity`
- `status = PartiallyFilled` or `Filled`

On each maker's `MarketUser`:

- `unsettled_quote += gross - fee` (maker was an ask)
- `unsettled_base += fill.fill_quantity` (maker was a bid)
- `open_orders` list: maker's order removed if fully filled

On `order_book`:

- `next_order_id += 1`
- Fully-filled makers removed from the relevant side (bids or asks) in
  reverse-index order
- Taker's remainder (if any) inserted into the correct side in price
  order

On the caller's new `order`:

- All fields populated
- `status = Filled` if taker fully matched; otherwise
  `PartiallyFilled` (if some fills) or `Open` (if no fills)

### 3.4 `cancel_order`

**Who calls it:** the order's owner.

**Signers:** `owner`.

**Accounts in:**

- `market`
- `order_book` (mut)
- `order` (mut, PDA seeds-checked via stored bump)
- `market_user` (mut)
- `owner` (signer)

**Checks:**

- `order.owner == owner.key()` → `Unauthorized`
- `order.status ∈ {Open, PartiallyFilled}` → `OrderNotCancellable`
- The order's `order_id` is present in `order_book` → `OrderNotFound`
  (sanity - shouldn't normally fire since fully-filled orders aren't
  cancellable)

**Token movements:** none. Cancellation is an accounting-only step.

**State changes:**

- For a cancelled bid: `unsettled_quote += price * remaining_quantity`
  (the quote the bid had locked in the vault is now owed back to the
  owner).
- For a cancelled ask: `unsettled_base += remaining_quantity`.
- Remove from `order_book.bids` or `order_book.asks`.
- Remove from `market_user.open_orders`.
- `order.status = Cancelled`.

The actual token move happens on the next `settle_funds` call.

### 3.5 `settle_funds`

**Who calls it:** any user. No-op when both unsettled counters are
zero, so it is safe to call on a heartbeat/cron.

**Signers:** `owner`.

**Accounts in:**

- `market` (mut)
- `market_user` (mut)
- `base_vault`, `quote_vault` (mut, boxed)
- `user_base_account`, `user_quote_account` (mut, boxed - caller's
  ATAs; caller must create them before calling)
- `base_mint`, `quote_mint` (boxed, read-only)
- `owner` (signer)
- `token_program`

**Checks:** none beyond Anchor's account-validation (ownership,
mint checks on token accounts, PDA seeds).

**Token movements:**

```
  base_vault  --[market_user.unsettled_base of base_mint]--> user_base_account
  quote_vault --[market_user.unsettled_quote of quote_mint]--> user_quote_account
```

Both transfers are CPIs to the Token program, signed by the
`Market` PDA using seeds `["market", base_mint, quote_mint, bump]`.

Order of operations is checks-effects-interactions: the
`unsettled_*` counters are zeroed *before* the transfer CPIs, then
the transfers run. Solana CPIs aren't reentrant in the EVM sense,
but zeroing state first means no future token-program extension or
transfer hook can observe stale unsettled balances mid-CPI and
double-withdraw.

**State changes:**

- `market_user.unsettled_base = 0`
- `market_user.unsettled_quote = 0`

### 3.6 `withdraw_fees`

**Who calls it:** the market authority (whichever pubkey was set as
`market.authority` at initialisation).

**Signers:** `authority`.

**Accounts in:**

- `market` (mut, `has_one = fee_vault`)
- `fee_vault` (mut, boxed)
- `authority_quote_account` (mut, boxed - destination)
- `quote_mint` (boxed)
- `authority` (signer)
- `token_program`

**Checks:**

- `authority.key() == market.authority` → `NotMarketAuthority`
- If `fee_vault.amount == 0`, returns `Ok(())` silently (so this call
  is cheap to schedule)

**Token movements:**

```
  fee_vault --[fee_vault.balance of quote_mint]--> authority_quote_account
```

Signed by the Market PDA.

**State changes:** none on program state (the vault balance drops to
zero as a side effect of the transfer).

---

## 4. The matching engine - step by step

This is the heart of the program. Everything in `place_order` after
the initial fund lock is matching-engine work. Follow along with
[`place_order.rs`](programs/order-book/src/instructions/place_order.rs) and
[`state/matching.rs`](programs/order-book/src/state/matching.rs) - it'll
read more easily once you've gone through this section.

### Ensuring fast order matching performance

The book must find the best-priced resting order on every `place_order`
call. Storing orders in a plain list (`Vec<Order>`) would work at small
scale, but finding the best price requires scanning every entry - in
formal notation that's **O(n)**: double the number of open orders,
double the work.

A [balanced binary search tree](https://en.wikipedia.org/wiki/Self-balancing_binary_search_tree)
keeps both sides of the book sorted at all times, so the best price is
always at the root. Worst-case lookup, insert, and delete are
**O(log₂ n)**: at 1 024 open orders per side that's at most 10 comparisons
instead of 1 024.

The specific data structure used here is a
[critbit tree](https://cr.yp.to/critbit.html) (short for *critical-bit
tree*) - a compact binary radix trie where each internal node splits on
the first bit where two keys disagree. Unlike a self-balancing BST it
never rotates or recolours nodes; its depth is instead bounded by the
*bit width of the key* rather than the number of orders, so it stays
shallow no matter what order keys arrive in. This implementation is ported from
[Openbook v2](https://github.com/openbook-dex/openbook-v2);
[Phoenix](https://github.com/Ellipsis-Labs/phoenix-v1) uses the same
approach. Both are production Solana CLOBs worth reading alongside this
example.

### 4.1 The plan

1. Caller passes `(side, price, quantity)` and, in remaining_accounts,
   the maker pairs to cross against.
2. The handler locks the required funds into the vault (done up
   front, before any matching - see §3.3).
3. **Plan the fills** (pure logic, no mutations): walk the opposite
   side of the book sorted by price (best price first). For each
   entry whose price
   crosses the taker's limit, record a `Fill { resting_index,
   resting_order_id, fill_quantity, fill_price }`. Stop when either
   the taker's quantity is exhausted or the next entry fails to
   cross.
4. **Apply the fills** (mutate state): for each fill, update the
   maker's `Order` (increment `filled_quantity`, flip status), update
   the maker's `MarketUser` (credit `unsettled_base` or
   `unsettled_quote`), and accumulate deltas for the taker.
5. **Clean the book**: remove fully-filled makers from the relevant
   side of `order_book.bids`/`asks`, in reverse-index order.
6. **Pay the fee**: one batched CPI from `quote_vault` to `fee_vault`
   for the sum of per-fill fees.
7. **Apply the taker deltas**: single mutation of the taker's
   `MarketUser`.
8. **Rest the remainder**: if `taker_remaining > 0`, insert the
   new `Order` into the book at the taker's limit price, add its
   `order_id` to the taker's `open_orders`, set status to
   `PartiallyFilled` (if any fills) or `Open` (if none).

### 4.2 Why bids spend quote, asks spend base - the full accounting

Pick a taker **bid** at price `bp` and quantity `bq`, crossing a
resting **ask** at `ap ≤ bp` with remaining quantity `aq`. Let
`fill_qty = min(bq, aq)` and `fill_price = ap` (maker's price wins).

Per-fill quantities:

```
gross       = fill_price * fill_qty                         (quote tokens)
fee         = ceil(gross * fee_bps / 10_000)                 (quote tokens)
net_to_maker = gross - fee                                   (quote tokens)
locked      = bp * fill_qty                                  (quote tokens the taker had locked for this fill)
rebate      = locked - gross                                 (quote the taker locked but doesn't need to spend)
```

Token flows:

```
  quote_vault  --[fee]---------> fee_vault       (CPI signed by Market PDA, batched across all fills)

  # No physical transfer for the base and net-quote legs - they stay in the
  # vaults, accounted for via unsettled_* counters:

  maker.unsettled_quote += net_to_maker          (maker collects gross - fee)
  taker.unsettled_base  += fill_qty              (taker gets the base)
  taker.unsettled_quote += rebate                (price improvement refund)
```

The *base* that the taker now owns was already in `base_vault` -
remember, the maker locked it there when placing the ask. The *quote*
that the maker now owns was already in `quote_vault` - the taker
locked `bp * bq` there at the top of this call. Nothing leaves the
vaults except the fee. Everything else gets paid out later, on
`settle_funds`.

For the opposite direction - a taker **ask** at `ap` crossing a
resting **bid** at `bp ≥ ap`:

```
fill_qty     = min(taker_remaining, bp_remaining)
fill_price   = bp
gross        = bp * fill_qty
fee          = ceil(gross * fee_bps / 10_000)
net_to_taker = gross - fee

Token flows:
  quote_vault --[fee]------> fee_vault

  taker.unsettled_quote += net_to_taker
  maker.unsettled_base  += fill_qty
```

No rebate on this side: the maker's bid locked exactly `bp *
bid_original_qty` of quote up front, and of that, `bp * fill_qty` is
being spent right now at exactly that price - no leftover.

### 4.3 Worked example - taker bid crosses two resting asks

Start with an empty book. Fees 10 bps (0.1%). Tick size 1.

1. Maker Dan posts an ask at price 900, quantity 5. `place_order(Ask,
   900, 5)`. Dan's token account loses 5 base; base_vault gains 5
   base. `order_book.asks = [(id=1, price=900)]`.

2. Maker Erin posts an ask at price 950, quantity 5. Same mechanism.
   `base_vault.balance = 10`. `order_book.asks = [(1, 900), (2, 950)]`
   (ascending).

3. Taker Faye places a bid at 1000 for quantity 7. She passes both
   makers as remaining_accounts: `(order_1, dan_user), (order_2,
   erin_user)`.

   Step A - lock. Faye's quote ATA loses `1000 * 7 = 7000` quote;
   `quote_vault.balance += 7000`.

   Step B - plan:
   - Fill 0: resting index 0 (Dan's ask), order_id 1, qty = min(7,
     5) = 5, price = 900. `taker_remaining = 7 - 5 = 2`.
   - Fill 1: resting index 1 (Erin's ask), order_id 2, qty = min(2,
     5) = 2, price = 950. `taker_remaining = 0`.

   Step C - apply fills:

   For Fill 0 (Dan):
   - gross = 900 * 5 = 4500; fee = ceil(4500 * 10 / 10 000) = ceil(4.5) = 5;
     net_to_maker = 4495.
   - `dan_market_user.unsettled_quote += 4495`
   - `faye_market_user.unsettled_base += 5`
   - Faye's rebate = 1000*5 − 4500 = 500.
     `faye_market_user.unsettled_quote += 500`
   - `dan_order.filled_quantity = 5`, status = Filled,
     remove from `dan_market_user.open_orders`.

   For Fill 1 (Erin):
   - gross = 950 * 2 = 1900; fee = ceil(1.9) = 2; net_to_maker = 1898.
   - `erin_market_user.unsettled_quote += 1898`
   - `faye_market_user.unsettled_base += 2`
   - Faye's rebate = 1000*2 − 1900 = 100.
     `faye_market_user.unsettled_quote += 100`
   - `erin_order.filled_quantity = 2`, status = PartiallyFilled
     (original 5, filled 2), **stays** in `erin_market_user.open_orders`.

   Step D - clean book. Dan's ask was fully filled → leaf removed from
   the asks critbit tree. Erin's ask was partially filled → leaf's
   `quantity` decremented in place to 3 (no tree rebalancing needed).
   The `Order` PDA carries `filled_quantity`; the leaf just holds the
   remaining quantity the matching engine needs to plan future fills.
   The next taker who wants to hit Erin's ask will pass `order_2` as a
   maker and see `leaf.quantity = 3`.

   Step E - pay the fee. `total_fee_quote = 5 + 2 = 7`. One CPI:
   ```
   quote_vault --[7 quote]--> fee_vault
   ```

   Step F - apply Faye's deltas. `faye_market_user.unsettled_base =
   0 + 7 = 7`. `faye_market_user.unsettled_quote = 0 + (500 + 100) =
   600`.

   Step G - rest the remainder. `taker_remaining = 0` → Faye's new
   Order is marked `Filled` immediately, not added to the book.

4. Later, each user calls `settle_funds`:
   - Dan's settle: `base_vault` loses 0 base; `quote_vault` loses
     4495 quote → Dan's quote ATA gains 4495.
   - Erin's settle: 1898 quote to Erin's ATA.
   - Faye's settle: 7 base to Faye's base ATA; 600 quote refund to
     Faye's quote ATA (unused from her 7000 lock).

5. At some point the market authority calls `withdraw_fees`:
   `fee_vault.balance = 7` → drained to authority's quote ATA.

**Post-settlement invariant check**:
- `base_vault.balance` should equal sum of remaining ask quantities =
  3 (Erin's remainder). ✓
- `quote_vault.balance` should equal sum of resting bids = 0. ✓

### 4.4 Partial fill with a remainder

Same scenario, but Faye bids at 920 (not 1000) and quantity 8.

- Fill 0: index 0 (Dan, 900), qty 5, price 900. Taker remaining 3.
- Attempt Fill 1: index 1 (Erin, 950). Crossing check: incoming bid at
  920, resting ask at 950 → `920 >= 950` is **false**. Matching
  stops.

After applying Fill 0 and the fee, `taker_remaining = 3 > 0`. The
book-capacity check runs (still fine). Faye's new Order is marked
`PartiallyFilled` (filled 5 of 8) and inserted into `order_book.bids`
at price 920. Her `open_orders` list now includes the new order_id.

Erin's ask was untouched; the book now looks like:

```
asks  [(2, 950)]        ← Erin, original 5 left
bids  [(3, 920)]        ← Faye, remaining 3
```

### 4.5 Cancel + settle round trip

Taker Gael places a bid at 910 for quantity 4 on an empty book (no
maker pairs passed). The bid rests.

- Step A (lock): `910 * 4 = 3640` quote moved from Gael's ATA to
  quote_vault. `order_book.bids = [(4, 910)]`.
- Step B–F: no fills, no fee, no maker mutations.
- Step G: `taker_remaining = 4 = quantity` → status `Open`, added
  to the book, `gael_market_user.open_orders = [4]`.

Gael decides to cancel. `cancel_order` on order_id 4:

- `remaining_quantity(order) = 4 - 0 = 4`.
- `gael_market_user.unsettled_quote += 910 * 4 = 3640`.
- `order_book.bids` cleared. `gael_market_user.open_orders = []`.
- `order.status = Cancelled`.

No tokens moved - `quote_vault.balance` still holds the 3640.

Gael calls `settle_funds`:

- `quote_vault --[3640 quote]--> gael_user_quote_account`
- `gael_market_user.unsettled_quote = 0`.

Net effect: Gael's balance sheet is exactly where it started; the
program earned nothing (no fill means no fee).

---

## 5. Full-lifecycle worked examples

Three scenarios with end-to-end numbers. Both mints are 6-decimal SPL
tokens. 1 BASE = 1 000 000 base units; 1 QUOTE = 1 000 000 quote
units. Where a number in the narrative looks like "price 900", read
that as "900 quote units per 1 base unit" (so for a 1-full-BASE trade
you'd move 900 * 1 000 000 quote units).

Market configuration:
- `fee_basis_points = 50` (0.5%)
- `tick_size = 1`
- `min_order_size = 1`
- `base_vault`, `quote_vault`, `fee_vault` all start empty.

### 5.1 A clean match: taker bid consumes a resting ask

Cast: **Maria** (market authority + Alice/Bob's broker), **Alice**
(seller), **Bob** (buyer).

1. `initialize_market` - Maria runs it. Rent for five accounts comes
   out of her wallet. Market is now `is_active`.
2. `create_market_user` - Alice and Bob each run it once.
3. Alice posts an ask: `place_order(Ask, 1000, 5)`, no
   remaining_accounts (empty book).
   - Lock: `alice_base_account --[5 base]--> base_vault`.
   - Plan: nothing to cross.
   - Rest: new Order PDA with `original_quantity = 5`, status `Open`,
     added to `order_book.asks` at index 0. `alice.open_orders = [1]`.
4. Bob posts a bid: `place_order(Bid, 1000, 5)`, with Alice's Order
   and MarketUser as remaining_accounts.
   - Lock: `bob_quote_account --[5 * 1000 = 5000 quote]-->
     quote_vault`.
   - Plan: one fill at (resting_index 0, order_id 1, qty 5, price
     1000).
   - Apply:
     - gross = 5000, fee = 5000 * 50 / 10 000 = 25, net_to_maker =
       4975.
     - `alice.unsettled_quote += 4975`
     - `bob.unsettled_base += 5`
     - Bob's rebate = 0 (he bid at the resting price exactly).
     - Alice's Order: filled 5, status Filled. Removed from
       `alice.open_orders`.
   - Clean book: drop index 0. `order_book.asks = []`.
   - Fee CPI: `quote_vault --[25 quote]--> fee_vault`.
   - Apply Bob's deltas.
   - Rest remainder: `taker_remaining = 0`, so Bob's new Order is
     marked Filled immediately, not booked.

**Balances at this point (in vault land):**
- `base_vault`: 5 base (waiting for Bob's settle).
- `quote_vault`: 4975 quote (waiting for Alice's settle). The other
  25 is now in fee_vault.
- `alice.unsettled_quote = 4975`, `alice.unsettled_base = 0`.
- `bob.unsettled_base = 5`, `bob.unsettled_quote = 0`.

5. Alice calls `settle_funds`:
   ```
   quote_vault --[4975 quote]--> alice_quote_account
   ```
   `alice.unsettled_quote = 0`.

6. Bob calls `settle_funds`:
   ```
   base_vault --[5 base]--> bob_base_account
   ```
   `bob.unsettled_base = 0`.

7. Maria calls `withdraw_fees`:
   ```
   fee_vault --[25 quote]--> maria_quote_account
   ```

**Final balance sheet (deltas from start):**
- Alice: −5 base, +4975 quote.
- Bob: +5 base, −5000 quote.
- Maria: +25 quote (minus whatever lamports she spent on rent for
  accounts).
- All three vaults empty.

### 5.2 Partial fill with remainder on the book

Cast: Alice (ask maker), Bob (bid maker, then remainder rests), Carol
(new taker).

1. `initialize_market` by Maria (same config).
2. `create_market_user` × 3.
3. Alice posts `Ask, 1000, 3`. Locks 3 base.
4. Bob posts `Bid, 1100, 10` with Alice's pair as a maker.
   - Lock: `10 * 1100 = 11_000 quote` from Bob to quote_vault.
   - Plan one fill: qty = min(10, 3) = 3, price = 1000.
   - gross = 3000, fee = 15, net_to_maker = 2985.
     - `alice.unsettled_quote += 2985`
     - `bob.unsettled_base += 3`
     - Rebate: `1100*3 − 3000 = 300` → `bob.unsettled_quote += 300`.
     - Alice's order fully filled.
   - Clean book: drop Alice's ask. `asks = []`.
   - Fee CPI: 15 quote to fee_vault.
   - `taker_remaining = 10 − 3 = 7`. Capacity OK. Bob's new Order
     marked PartiallyFilled (filled 3 of 10), added to
     `order_book.bids` at price 1100. `bob.open_orders = [2]`.

   Book state now: `asks=[], bids=[(2, 1100)]`. `quote_vault` holds
   the locked portion for Bob's remainder:
   `11000 − (3000 + 300 + 2985) = 4715`? Let's double-check: 2985 is
   *inside* quote_vault (alice's unsettled). 300 is *inside*
   quote_vault (bob's rebate unsettled). 15 went to fee_vault. 3000
   minus fee = 2985 net_to_maker sits in quote_vault waiting for
   Alice's settle. So `quote_vault.balance = 11000 − 15 = 10985`,
   composed of: alice.unsettled_quote (2985) + bob.unsettled_quote
   (300) + bob's remaining lock for the resting bid (1100 * 7 =
   7700). 2985 + 300 + 7700 = 10 985. ✓

5. Alice settles: `quote_vault --[2985]--> alice_quote_account`.
   `quote_vault = 10985 − 2985 = 8000` (= 7700 Bob-lock + 300
   Bob-rebate).
6. Carol posts `Ask, 1100, 4` with Bob's Order/MarketUser as a
   maker pair.
   - Lock: 4 base from Carol to base_vault.
   - Plan: fill at (index 0, order_id 2, qty min(4, 7) = 4, price
     1100).
   - gross = 4400, fee = 22, net_to_taker = 4378.
     - `carol.unsettled_quote += 4378`
     - `bob.unsettled_base += 4` (he's the maker-bid; base flows to
       the bid side)
     - No rebate on ask-taker side.
     - Bob's order: filled_quantity 3 → 7, status PartiallyFilled
       (still not fully filled - original 10, filled 7).
   - Clean book: Bob's book remaining = 10 − 7 = 3 > 0, so his
     entry stays. `order_book.bids = [(2, 1100)]`.
   - Fee CPI: 22 quote → fee_vault.
   - `taker_remaining = 0` → Carol's new Order marked Filled.

   Mid-state: `base_vault = 0 + 4 = 4` (from Carol's lock; was 0
   after Bob's settle made it flow - wait, no: Bob's base never
   settled yet. Let's re-check:)

   After step 4 Bob's `unsettled_base = 3` (from the 3-base fill
   against Alice). `base_vault.balance = 3 + 0 = 3` (Alice's
   original lock after the fill; asks had drained out with the
   match). After step 6, Carol added 4 base and 4 went to Bob as
   unsettled. So `base_vault.balance = 3 + 4 = 7`. `bob.unsettled_base
   = 3 + 4 = 7`.

### 5.3 Cancel round-trip

Cast: Alice (bid maker), nobody else.

1. `initialize_market`, `create_market_user(Alice)`.
2. Alice posts `Bid, 900, 10` - rests on an empty book.
   - Lock: 9000 quote from Alice to quote_vault.
   - No fills. `alice.open_orders = [1]`. `bids = [(1, 900)]`.
3. Alice reconsiders and calls `cancel_order` on her bid.
   - `remaining_quantity = 10 − 0 = 10`.
   - `alice.unsettled_quote += 900 * 10 = 9000`.
   - `bids = []`, `alice.open_orders = []`.
   - `order.status = Cancelled`.
4. Alice calls `settle_funds`:
   ```
   quote_vault --[9000 quote]--> alice_quote_account
   ```
   `alice.unsettled_quote = 0`.

Net delta: Alice is exactly where she started. The vaults are empty.
The Order account is still onchain in `Cancelled` state (one could
imagine a future instruction handler to reclaim its rent - see §8).

---

## 6. Safety and edge cases

### 6.1 What the program refuses to do

From [`errors.rs`](programs/order-book/src/errors.rs):

| Error | When |
|---|---|
| `InvalidPrice` | `place_order` called with `price == 0` |
| `OrderNotFound` | `cancel_order` failed to locate the order in the book (sanity path) |
| `MarketPaused` | `place_order` on a market with `is_active = false` (no handler flips this today, but the field is there) |
| `Unauthorized` | `cancel_order` by someone other than the order owner |
| `OrderBookFull` | `place_order` remainder would push the taker's side past 1024 leaves |
| `TooManyOpenOrders` | User already has 20 open orders on this market |
| `InvalidTickSize` | `tick_size == 0` at init, or `price % tick_size != 0` on place |
| `BelowMinOrderSize` | `min_order_size == 0` at init, or `quantity < min_order_size` on place |
| `OrderNotCancellable` | `cancel_order` on a Filled or Cancelled order |
| `NumericalOverflow` | Any checked arithmetic returned `None` |
| `InvalidFeeBasisPoints` | `fee_basis_points > 10_000` at init |
| `InvalidFeeVault` | `market.fee_vault` on the struct does not match the passed `fee_vault` (Anchor `has_one`) |
| `MakerAccountMismatch` | Wrong number of maker accounts, wrong order, wrong market, or caller walked the book out of order |
| `MissingMakerAccounts` | `remaining_accounts.len()` not a multiple of 2 |
| `MakerOwnerMismatch` | Maker Order and MarketUser have different owners |
| `NotMarketAuthority` | `withdraw_fees` called by wrong signer |

### 6.2 Guarded design choices worth knowing

- **Full lock on place.** The handler always moves the full locked
  amount into the vault before matching. This keeps the
  vault-balance invariant simple and makes `cancel_order` / partial
  fills straightforward: the vault already has everything it could
  owe.

- **Caller supplies maker pairs.** The matching engine does not
  iterate the whole book looking for counterparties - the caller
  tells it which resting orders to cross. This is what Openbook v2
  does and it's the only way to fit the matching work within a
  transaction's account budget when the book is large. The cost is
  that an off-book client needs to read the `OrderBook` account
  first, pick the crossings, and pass the right accounts. The
  program still enforces order (price-time priority) and ownership
  on what the caller passes, so a malicious caller cannot cross a
  non-top-of-book maker to hurt someone else - they can only *fail
  to cross* orders they should have crossed, which only hurts
  themselves.

- **Matching applies at the maker's price, not the taker's.** The
  fill price is always the resting order's price. Takers that cross
  deeper into the book get price improvement, refunded to
  `unsettled_quote` (for taker bids). This is the standard
  order-book rule.

- **Fees come out of the gross.** The maker receives `gross - fee`,
  not `gross`; the fee lives on for a while in `quote_vault` before
  being moved to `fee_vault` in one batched CPI at the end of
  `place_order`. An alternative model - the taker paying `gross +
  fee` on top of the lock - is discussed in a comment in
  `place_order.rs` and left as an exercise.

- **Unsettled balances are pure accounting.** No token physically
  moves to or from a user during matching or cancellation. Both
  events just bump `unsettled_*` counters. The user collects by
  calling `settle_funds`. This means one `place_order` call that
  crosses many makers only costs one token CPI (the fee move), not
  one-per-fill. Large orders stay within the CU budget.

- **`settle_funds` no-ops on zero.** Both legs are guarded by `if
  base_amount > 0` / `if quote_amount > 0`. Safe to schedule on a
  cron or heartbeat.

- **`withdraw_fees` no-ops on empty.** Likewise.

- **Boxed InterfaceAccounts.** Several handlers use `Box<
  InterfaceAccount<...>>` for mint/token accounts. That's a BPF
  stack-size workaround - each `InterfaceAccount` is ~1 KB on the
  stack and the Solana VM gives handlers a tight budget. Don't
  unbox these without testing the compute output size.

- **Discriminator + `has_one`.** Every state account carries an 8-
  byte discriminator that Anchor checks. `Market` has
  `has_one = fee_vault`, so the `place_order` handler can trust the
  `fee_vault` account without re-checking its mint or authority.

- **Book capacity check after matching.** The taker's remainder
  check happens at the end. A bid that clears enough asks to free
  up 3 slots can then rest its own 1-slot remainder even on a
  previously-full book - matching the "liquidity-positive" spirit
  of an order book.

### 6.3 Things this example does *not* do

A production order book would add:

- **Rent reclamation on `cancel_order`.** Cancelled `Order` accounts
  persist onchain indefinitely; a `close_order` instruction would let
  owners reclaim that rent after a cancel.
- **Cancel-on-expiry / GTC vs IOC vs FOK.** All orders here are
  implicitly GTC (good 'til cancelled).
- **Post-only / reject-if-cross.** No way to guarantee your order
  will be a maker.
- **Self-trade protection.** Nothing stops a single user from
  crossing their own resting order.
- **Rent reclamation for closed orders.** `Order` accounts persist
  onchain in `Filled` or `Cancelled` state forever; a real program
  would either close them in the same handler or provide a
  `close_order` to reclaim rent later.
- **Partial taker-funded fees.** The fee comes out of the maker's
  gross today (see `place_order.rs` comment). If you want
  maker-neutral fees, take an additional transfer from the taker's
  ATA at match time.
- **Minimum-tick for quantities.** `min_order_size` is a floor, but
  there's no "round lot" constraint.
- **Pause / admin / upgrade.** `is_active` exists but no handler
  flips it.
- **Oracle-aware price bands.** A taker bid 10 000× higher than the
  best ask will happily sweep the book.

---

## 7. Running the tests

All tests are LiteSVM Rust integration tests under
[`programs/order-book/tests/test_order_book.rs`](programs/order-book/tests/test_order_book.rs).
They load the built `.so` via
`include_bytes!("../../../target/deploy/order_book.so")`, so a build must
run first.

### Prerequisites

- Anchor 1.0.0
- Solana CLI (`solana -V`)
- Rust stable (pinned at the repo root)

### Commands

From `finance/order-book/anchor/`:

```bash
# 1. Build the .so - target/deploy/order_book.so
anchor build

# 2. Run the LiteSVM tests
cargo test --manifest-path programs/order-book/Cargo.toml

# Or equivalently (Anchor.toml scripts.test = "cargo test"):
anchor test --skip-local-validator
```

Expected:

```
running 23 tests
test authority_can_withdraw_fees_after_match ... ok
test cancel_and_settle_bid_refunds_full_quote ... ok
test cancel_ask_credits_unsettled_base ... ok
test cancel_order_rejects_non_owner ... ok
test create_market_user_tracks_market_and_owner ... ok
test fee_vault_receives_exactly_bps_of_taker_gross ... ok
test initialize_market_rejects_oversized_fee ... ok
test initialize_market_rejects_zero_tick_size ... ok
test initialize_market_sets_market_and_order_book ... ok
test place_ask_locks_base_in_vault ... ok
test place_bid_locks_quote_in_vault ... ok
test place_order_rejects_below_min_order_size ... ok
test place_order_rejects_unaligned_tick ... ok
test place_order_rejects_zero_price ... ok
test resting_orders_at_same_price_fill_by_time_priority ... ok
test settle_funds_after_match_pays_out_both_unsettled_balances ... ok
test settle_funds_moves_unsettled_base_to_user ... ok
test taker_ask_fully_crosses_best_bid ... ok
test taker_bid_fully_crosses_best_ask ... ok
test taker_bid_gets_price_improvement_from_resting_ask ... ok
test taker_crosses_multiple_resting_orders_best_price_first ... ok
test taker_partially_filled_remainder_rests_on_book ... ok
test taker_partially_fills_resting_order_rest_stays_on_book ... ok
```

### What each test exercises

**Setup / happy path (pre-matching):**

| Test | Exercises |
|---|---|
| `initialize_market_sets_market_and_order_book` | PDA creation, vault setup, initial field values |
| `create_market_user_tracks_market_and_owner` | Per-user PDA derivation and zero-initialised counters |
| `place_bid_locks_quote_in_vault` | Fund lock on bid |
| `place_ask_locks_base_in_vault` | Fund lock on ask |
| `settle_funds_moves_unsettled_base_to_user` | Vault → user ATA transfer via market PDA signer |

**Validation:**

| Test | Exercises |
|---|---|
| `place_order_rejects_zero_price` | `price > 0` |
| `place_order_rejects_unaligned_tick` | `price % tick_size == 0` |
| `place_order_rejects_below_min_order_size` | `quantity >= min_order_size` |
| `cancel_order_rejects_non_owner` | Ownership check on cancel |
| `initialize_market_rejects_zero_tick_size` | Init constraint |
| `initialize_market_rejects_oversized_fee` | `fee_bps <= 10_000` |

**Cancel + settle flow:**

| Test | Exercises |
|---|---|
| `cancel_ask_credits_unsettled_base` | Ask cancel → `unsettled_base += remaining` |
| `cancel_and_settle_bid_refunds_full_quote` | Round trip of a Bob-style cancellation |

**Matching engine:**

| Test | Exercises |
|---|---|
| `taker_bid_fully_crosses_best_ask` | Full-fill crossing, fee routed correctly |
| `taker_ask_fully_crosses_best_bid` | Symmetric path |
| `taker_partially_fills_resting_order_rest_stays_on_book` | Resting order's `filled_quantity` updated, not removed |
| `taker_partially_filled_remainder_rests_on_book` | Taker's remainder inserted in correct price order |
| `taker_crosses_multiple_resting_orders_best_price_first` | Walks multiple makers in price priority |
| `resting_orders_at_same_price_fill_by_time_priority` | Tie-break at same price is first-in-first-out |
| `taker_bid_gets_price_improvement_from_resting_ask` | Rebate → `unsettled_quote` |
| `fee_vault_receives_exactly_bps_of_taker_gross` | Fee math in a single batched CPI |
| `authority_can_withdraw_fees_after_match` | Fee drain after fills, authority-gated |
| `settle_funds_after_match_pays_out_both_unsettled_balances` | Both legs paid in one call |

### CI note

The repo's `.github/workflows/anchor.yml` runs `anchor build` before
`anchor test` for every changed anchor project. That matters here:
the integration tests include the BPF artefact via `include_bytes!`,
so a stale or missing `.so` would break the tests. CI is already
covered.

---

## 8. Extending the program

Ordered by difficulty.

### Easy

- **Close-on-terminal `Order`.** After a `place_order` fully fills a
  maker, close its `Order` account in the same handler and refund
  rent to the owner. Same for `cancel_order` on an `Open` order.
  Saves onchain storage.

- **IOC flag.** Add `post_only: bool` and `ioc: bool` parameters.
  `ioc` means "match what you can and discard the remainder instead
  of resting it". `post_only` means "reject the order if it would
  cross". Both are one-line checks around the existing matching
  logic.

- **Self-trade guard.** Reject a fill where `maker_order.owner ==
  owner.key()`. Alternative: auto-cancel the maker side.

### Moderate

- **Taker-funded fees.** Pull the fee from the taker's ATA in a
  second transfer at match time, instead of netting it out of the
  maker's gross. Preserves strict "maker pays nothing" semantics.

- **Order expiry.** Add `expires_at: i64` to `Order`. In
  `place_order`, skip resting entries whose `expires_at` is past;
  add a permissionless `sweep_expired` instruction.

### Why a depth-bounded tree (critbit)?

**Worst-case depth must be bounded, not assumed.** A plain binary
search tree only keeps a roughly-balanced shape when its inputs arrive
in random order. In an order book an attacker chooses the inputs - the
prices of their orders - so nothing they choose can be allowed to
inflate the tree's depth. Two families of structure defend against
this: *self-balancing* BSTs (red-black, AVL, …) that restore a bounded
height with rotations on every insert and delete, and *radix tries*
like critbit whose depth is capped by the key's bit width no matter
which keys are present. Both keep every operation cheap regardless of
input order; this example uses the second.

**Concrete attack on a plain BST.** An attacker posts orders at
monotonically increasing prices ($100, $101, $102, $103, …). Each new
price is greater than every previous one, so each new node attaches as
the right child of the previous one. After N such orders the tree has
degenerated into a linked list of length N. Lookups, inserts, and
matches all walk O(N) instead of O(log N).

**Why this matters on Solana specifically.** Solana transactions have
a ~1.4M compute-unit budget. If `place_order` walks a degenerate book
and exceeds the CU limit mid-match, the transaction aborts and the
placer pays fees for nothing. Worse, *legitimate users' orders fail
because an adversary skewed the tree shape*. A depth-bounded tree keeps
every operation cheap regardless of input, so the attack is
structurally impossible.

**Why critbit specifically.** Critbit is a binary radix trie keyed on
the order's sort bits - *not* a self-balancing BST, so it never rotates
or recolours nodes. Its shape is a deterministic function of which keys
are present, and its depth can never exceed the *bit width of the sort
key* (128 bits here - price in the high 64, sequence number in the low
64), so it cannot degenerate into a long chain under any insert order.
An insert splits exactly one leaf and adds exactly one inner node; a
delete splices one out. This example uses the critbit slab from
Openbook v2 (`src/state/slab/`).

### Harder

- **Event queue.** Mirror Openbook's `EventQueue` - `place_order`
  writes "fill" events, and a separate `consume_events` instruction
  processes them in batches for the maker side. Makes matching O(1)
  in CU cost regardless of the taker's depth.

- **Market-makers as CPI users.** Formalise the `remaining_accounts`
  protocol so a market-making program can call `place_order` on
  behalf of its users, pre-computing the crossings offchain and
  rewriting the book in one transaction.

- **Cross-market swaps.** Chain two `place_order` calls (e.g.
  base→USDC then USDC→quote2) with an outer helper that routes
  through `unsettled_*` balances without a settle in between.

---

## Code layout

```
finance/order-book/anchor/
├── Anchor.toml
├── Cargo.toml
├── README.md              (this file)
└── programs/order-book/
    ├── Cargo.toml
    ├── src/
    │   ├── errors.rs
    │   ├── lib.rs         #[program] entry points
    │   ├── instructions/
    │   │   ├── mod.rs
    │   │   ├── initialize_market.rs
    │   │   ├── create_market_user.rs
    │   │   ├── place_order.rs        (matching engine lives here)
    │   │   ├── cancel_order.rs
    │   │   ├── settle_funds.rs
    │   │   └── withdraw_fees.rs
    │   └── state/
    │       ├── mod.rs
    │       ├── market.rs
    │       ├── order.rs
    │       ├── order_book.rs         (critbit wrappers, allocation, apply_fill)
    │       ├── market_user.rs
    │       ├── matching.rs           (pure fill-planning logic)
    │       └── slab/                 (critbit tree, ported from Openbook v2)
    │           ├── mod.rs
    │           ├── nodes.rs
    │           ├── ordertree.rs
    │           └── iterator.rs
    └── tests/
        └── test_order_book.rs              LiteSVM tests
```
