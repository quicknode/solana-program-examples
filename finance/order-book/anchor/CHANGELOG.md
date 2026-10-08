# Changelog

## Unreleased - 2026-10-07

### Fixed

- The `place_order` doc comment said ties at one price break on the
  earliest timestamp. They break on the lowest order id, the order book's
  sequence number, which is what the matching code compares. Comment
  only; no behavior changes.

## Unreleased - 2026-10-05

### Added

- `close_order` and `close_market_user`, so a participant's rent comes
  back. Nothing closed an `Order` or `MarketUser` account before, so every
  order and every registration cost its owner the rent for good.
  `close_order` closes a Filled or Cancelled order to its owner, who signs;
  both statuses have already left the slab and the owner's `open_orders`,
  and a cancel has already credited its refund, so nothing refers to the
  account. An Open or PartiallyFilled order gets the new `OrderNotClosable`.
  `close_market_user` closes the owner's `MarketUser` when `open_orders` is
  empty and both unsettled balances are zero, else the new
  `MarketUserNotClosable`; the owner can register again afterwards. Either
  signed by anyone but the owner gets `Unauthorized`. Tests:
  `close_order_returns_a_cancelled_orders_rent`,
  `close_order_returns_a_filled_orders_rent`,
  `close_order_refuses_a_resting_order`,
  `close_order_refuses_a_partially_filled_order`,
  `close_order_refuses_a_non_owner`,
  `close_market_user_returns_rent_when_nothing_is_open_or_owed`,
  `close_market_user_refuses_an_open_order`,
  `close_market_user_refuses_an_unsettled_balance`,
  `close_market_user_refuses_a_non_owner`.
- `withdraw_fees_rejects_a_non_authority_signer`: after a fill, a trader
  signing `withdraw_fees` gets `NotMarketAuthority`, the fee stays in the
  vault, and the trader's quote balance is unchanged.

### Changed

- The test suite's comment on the shared bid and ask sizing no longer says
  matching is not implemented. It is; no test places both orders, so they
  never cross.
- Every refusal test asserts the error code the call fails with, through
  the suite's `failure_text` and `assert_fails_with` helpers:
  `place_order_rejects_zero_price` (`InvalidPrice`),
  `place_order_rejects_unaligned_tick` (`InvalidTickSize`),
  `place_order_rejects_below_min_order_size` (`BelowMinOrderSize`),
  `cancel_order_rejects_non_owner` (`Unauthorized`),
  `settle_funds_rejects_fee_vault_substituted_for_quote_vault`
  (`InvalidQuoteVault`), `initialize_market_rejects_zero_tick_size`
  (`InvalidTickSize`), `initialize_market_rejects_zero_base_lot_size`
  (`InvalidBaseLotSize`), `initialize_market_rejects_zero_quote_lot_size`
  (`InvalidQuoteLotSize`) and `initialize_market_rejects_oversized_fee`
  (`InvalidFeeBasisPoints`).
- `build_withdraw_fees_ix` takes the signing authority, as the pause and
  resume builders do.

## Unreleased - 2026-10-04

### Added

- `pause_market` and `resume_market`, signed by the market authority
  (anyone else gets `NotMarketAuthority`). `pause_market` clears
  `Market.is_active`, which `initialize_market` set and nothing cleared, so
  the `MarketPaused` refusal in `place_order` can now happen.
  `resume_market` sets the flag again. A pause stops new orders and nothing
  else: `cancel_order`, `settle_funds` and `withdraw_fees` do not read the
  flag. Tests: `pause_market_refuses_new_orders_with_market_paused`,
  `paused_market_still_cancels_and_settles_a_resting_order`,
  `paused_market_still_pays_out_fills_and_withdraws_fees`,
  `resume_market_accepts_orders_again`,
  `only_the_market_authority_can_pause_or_resume`.

### Changed

- `NotMarketAuthority`'s message names all three handlers it guards.
- The README's code layout shows `withdraw_fees.rs` under
  `instructions/admin/`, where it is.

## 2026-10-03

### Added

- `full_side_cancel_of_the_last_scanned_order_fits_the_default_budget`:
  `cancel_order` finds an order by walking its side from the best price, so
  on a full side of 512 bids the worst bid is the last one it reads. Canceling
  it stays inside the default 200,000-unit instruction budget.

## 2026-09-23

### Added

- `doubling_prices_build_the_deepest_path_prices_allow` and
  `deepest_path_adds_little_compute_to_insert_fill_and_cancel`: asks at 63
  doubling prices build a 64-level path to the best ask, and inserting,
  filling, and canceling at the bottom of it each cost less than 15,000
  compute units more than on a shallow book.
- A full side of the book evicts instead of refusing. When a side already
  holds its 512 orders, an order that beats the side's worst price removes
  that worst order and rests in its place. The evicted order is refunded
  through its owner's unsettled balance, as a cancel is, and stamped
  Cancelled. An order that does not beat the worst price still gets
  `OrderBookFull`. The caller passes the worst order and its owner's
  `MarketUser` after the maker pairs, or only the order when it is their
  own. New errors: `MissingEvictedAccounts`, `EvictedAccountMismatch`.

### Changed

- The README states the critbit tree's real depth bound (64 levels from
  prices, 128 at most) instead of saying it stays shallow whatever order keys
  arrive in, and says Phoenix uses a red-black tree.

### Fixed

- A side holds 512 orders, not 1024: every order after the first adds a
  leaf and an inner node to the side's 1024-node tree. `MAX_ORDERS_PER_SIDE`
  and the README now say so.

## 2026-09-22

### Changed

- The base, quote, and fee vaults are PDAs of the market, at seeds
  `["base_vault", market]`, `["quote_vault", market]` and
  `["fee_vault", market]`, instead of token accounts at public keys the client
  generated. Clients derive the vault addresses instead of generating and
  signing with three extra keys. The market still records each address and
  every handler still checks the vaults it is passed against that record.
  The order book stays a client-allocated account: at about 180 KB it is too
  large for the program to create. Tests derive the vaults instead of
  generating them.

## 2026-07-07

Added this changelog. Changes prior to this date were tracked in git history only.
