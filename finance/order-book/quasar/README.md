# Order Book: Central Limit Order Book (CLOB), Quasar port

A [central limit order book (CLOB)](https://www.investopedia.com/terms/l/limitorderbook.asp), the market
structure NYSE, NASDAQ, CME, and onchain venues like Phoenix and OpenBook run on. Users post buy or sell
offers at prices they pick; the program matches crossing offers in **price-time priority** and settles the
resulting token movements.

This is a [Quasar](https://github.com/blueshift-gg/quasar) port of the Anchor example in
[`../anchor`](../anchor). Quasar is a zero-copy, `no_std`, zero-allocation Solana framework with Anchor-like
syntax (`#[program]`, `#[derive(Accounts)]`, `#[account]`) that compiles to a much smaller binary. Both builds
use the **same program ID** (`C69UJ8irfmHq5ysyLek7FKApHR86FBeupiz4JnoyPzzx`), so offchain tooling and PDA
derivations work against either unchanged. The Anchor README carries the full conceptual walkthrough; this one
focuses on how the program works and what the Quasar port does differently.

## What the program does

Two users want to swap tokens at prices they each chose. **Alice** holds USDC (the *quote* mint, the pricing
unit) and wants to buy NVDAx (the *base* mint, the asset being priced), but only at 900 USDC/share or lower.
**Bob** holds NVDAx and will sell at 900 or higher. They post a **bid** and an **ask**; when the prices cross,
the program fills them.

- The party whose order **rests** on the book first is the **maker**.
- The party whose order arrives and **crosses** the resting order is the **taker**.

A fill always clears at the **maker's** posted price (standard CLOB rule): the taker gets price improvement
versus their own limit. The taker pays a fee (in basis points) routed to a fee vault; the maker's proceeds are
credited net of that fee.

Funds are held in **program-owned vaults** (the market PDA is their token authority) from the moment an order
locks them until settlement. There is no admin escape hatch: the market authority can only withdraw
accumulated **fees**, never user balances. The deployed program bytecode is the only thing that can move vault
funds, and it moves them only along the place / cancel / settle paths below.

## Accounts and PDAs

- `Market` (PDA, seeds `["market", base_mint, quote_mint]`): One trading pair. Stores config + vault addresses. Its PDA is the vaults' token authority.
- `OrderBook` (keypair account, not a PDA): Two critbit slabs (bids + asks), ~180 KB. Zero-copy. Bound to its market by the market's stored `order_book`.
- `MarketUser` (PDA, seeds `["market_user", market, owner]`): Per-user, per-market. Tracks open order ids and `unsettled_*` balances owed back to the user.
- `Order` (PDA, seeds `["order", market, order_id]`): One order. `order_id` is the book's monotonic counter at placement time.
- `base_vault` / `quote_vault` (token accounts): Hold locked funds while orders are open. Market PDA is the authority.
- `fee_vault` (token account): Accumulates taker fees (quote mint). Kept separate so user balances and fees can't be confused.

The order book is **not** a PDA. Solana caps inner-CPI account allocations at 10 KB, so a ~180 KB account can't
be created with an `init` constraint: the client calls `system_program::create_account` directly (sizing it to
`ORDER_BOOK_ACCOUNT_SIZE`, program-owned, zeroed) and passes it to `initialize_market`, which verifies and
initializes it in place.

## The two-lot pricing model

Both sides of the book are denominated in **lots**, mirroring Serum/OpenBook, so `price` and `quantity` read
as human numbers regardless of each mint's decimals:

```
raw_base  = quantity × base_lot_size
raw_quote = quantity × price × quote_lot_size
```

Choose `base_lot_size = 10^max(d_base − d_quote, 0)` and `quote_lot_size = 10^max(d_quote − d_base, 0)`. For
NVDAx (9 decimals) / USDC (6 decimals): `base_lot_size = 1000`, `quote_lot_size = 1`, so `price = 100` means
100 USDC-units per base lot and `tick_size = 1` is one atomic increment.

## Instruction lifecycle

- `initialize_market`: Create the `Market` PDA, the two vaults, and the fee vault; initialize the pre-created order-book account.
- `create_market_user`: Create a caller's `MarketUser` for a market.
- `place_order`: Lock funds, cross the opposing side in price-time priority, credit fills to maker/taker `unsettled_*`, route the taker fee, and rest any remainder.
- `cancel_order`: Credit an open order's locked remainder back to the owner's `unsettled_*` and remove it from the book.
- `settle_funds`: Move a user's `unsettled_*` balances out of the vaults into their token accounts.
- `withdraw_fees`: Authority-only: drain the fee vault to the authority's token account.

`place_order` takes `side` (`0` = bid, `1` = ask), `price`, `quantity`, and `order_id`. The caller passes the
resting maker orders to cross as **remaining accounts**, in pairs of `(maker_order, maker_market_user)`, in the
book's price-time priority. `order_id` must equal the book's current `next_order_id` (the program verifies it),
so the client derives the `Order` PDA deterministically.

Fills never transfer tokens directly to the counterparty; they credit `unsettled_*` balances that each user
drains later via `settle_funds`. This keeps the per-fill account footprint small (no maker ATAs in the fill
path), as in OpenBook v2.

## The matching engine

Each side of the book is a **critbit tree** (radix trie) ported from
[openbook-v2](https://github.com/openbook-dex/openbook-v2) (MIT; see `src/state/slab/LICENSE-OPENBOOK`). Leaves
carry the resting order's price, remaining quantity, owner, and id; a 128-bit key packs `[price][seq_num]` so an
in-order walk yields best-price-first, and within a price level the sequence number preserves time priority.
Insert, remove, and best-price lookup are all sub-linear, so matching stays cheap as depth grows.

## What the Quasar port does differently

The trading logic, fee model, lot math, and matching engine are identical to the Anchor build. The differences
are all consequences of Quasar being zero-copy, `no_std`, and zero-allocation:

- **The order book is accessed by casting raw account bytes.** Anchor uses `AccountLoader<OrderBook>`; Quasar
  has no `AccountLoader`, so `initialize_market` / `place_order` / `cancel_order` cast the account's byte region
  to `&mut OrderBook` with `bytemuck` (an 8-byte discriminator guards against a wrong account). The slab code is
  otherwise a near-verbatim port.
- **`MarketUser.open_orders` is a fixed `[u8; 160]` (20 packed u64s) plus a length**, not a borsh `Vec<u64>`. A
  resizable field would make the account dynamic, which is awkward to mutate on the maker side where the account
  arrives as a remaining account. The fixed buffer keeps every mutation an in-place write.
- **The slab is heap-free.** Its transient traversal stack is a fixed array (a critbit tree over 128-bit keys is
  at most 128 nodes deep), and two write-only stacks the upstream carried were dropped. A single `place_order`
  crosses at most `MAX_FILLS` (16) resting orders; any remainder rests on the book for a later order to cross.
- **`side` and order status/side enums are stored as `u8`** (zero-copy accounts hold POD scalars). The `u8`
  wire encoding of `side` matches the Anchor build's borsh enum discriminant.

## Safety and custody

- Every vault transfer out is signed by the **market PDA** via `invoke_signed`; only the deployed program can
  move locked funds.
- `settle_funds` zeroes a user's `unsettled_*` **before** transferring (checks-effects-interactions), so no
  token-hook re-entry could double-withdraw.
- `place_order` binds every market-owned account (`base_vault`, `quote_vault`, `fee_vault`, both mints, the
  order book) to the addresses stored on the `Market` PDA with `has_one`, so a caller can't substitute the fee
  vault for a user vault and drain fees.
- Taker fees use **ceiling** division, rounding in the protocol's favor so many tiny fills can't leak a minor
  unit to the maker.

## Building and testing

Requires the [Solana toolchain](https://docs.anza.xyz/cli/install) (for `cargo build-sbf`) and the
[Quasar CLI](https://github.com/blueshift-gg/quasar):

```sh
cargo install --git https://github.com/blueshift-gg/quasar quasar-cli --locked
quasar build          # compiles the program to target/deploy/quasar_order_book.so
cargo test            # QuasarSVM integration tests (they load the compiled .so)
```

`quasar build` must run before `cargo test`: the tests load the compiled `.so` into
[QuasarSVM](https://github.com/blueshift-gg/quasar-svm), an in-process SVM, via `include`/`fs::read`. The suite
in `src/tests.rs` drives the full lifecycle (initialize a market, create users, rest an ask, cross it with a
bid, settle both sides, and withdraw the fee), asserting onchain state, token balances, and fee accounting at
each step, plus an authorization rejection.

## Extending

- **Self-trade prevention:** reject or cancel-back when a taker would cross their own resting order.
- **Post-only / IOC / FOK** order types by gating the rest-vs-cancel behaviour on a flag.
- **Multiple fee tiers** keyed on the taker's `MarketUser`.
- **Market pause/resume** by flipping `Market.is_active` from an authority-gated handler.
