# Changelog

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
