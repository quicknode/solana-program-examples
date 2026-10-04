# Changelog

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

## 2026-09-23

### Added

- A full side of the book evicts instead of refusing. When a side already
  holds its 512 orders, an order that beats the side's worst price removes
  that worst order and rests in its place. The evicted order is refunded
  through its owner's unsettled balance, as a cancel is, and stamped
  Cancelled. An order that does not beat the worst price still gets
  `OrderBookFull`. The caller passes the worst order and its owner's
  `MarketUser` after the maker pairs, or only the order when it is their
  own. New errors: `MissingEvictedAccounts`, `EvictedAccountMismatch`.

### Fixed

- A side holds 512 orders, not 1024: every order after the first adds a
  leaf and an inner node to the side's 1024-node tree. `MAX_ORDERS_PER_SIDE`
  now says so.

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

## [2026-07-22]

### Changed

- Migrated to Quasar 0.1.0 (`0.1.0-release` branch, rev `be60fca`): Quasar.toml
  rewritten to the 0.1.0 schema, `idl-build` feature and `lib` crate-type added,
  and tests rewritten from the direct QuasarSVM harness to `quasar-test`
  (`#[quasar_test]` fixtures, `crate::cpi` instruction builders — including
  `remaining_accounts` for crossing maker orders — and `Outcome` assertions).
  The `quasar-svm` git dev-dependency is gone; compute-unit assertions were
  dropped pending recalibration under 0.1.0. Program-source fix for 0.1.0:
  `Seed` is no longer in the prelude, so `place_order.rs`, `settle_funds.rs`,
  and `admin/withdraw_fees.rs` now import it from `quasar_lang::cpi`.

## 2026-07-07

Added this changelog. Changes prior to this date were tracked in git history only.
