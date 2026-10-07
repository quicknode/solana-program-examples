# Changelog

## Unreleased, 2026-10-07

Add `close_market` (discriminator 5). The market account and its two vaults
had no close handler, so the rent the operator paid for all three at
`initialize_market` could never be recovered. The new handler is signed by the
operator, under the same `has_one(operator)` constraint as
`withdraw_inventory`; it refuses with the new `INVENTORY_NOT_EMPTY` (15) while
either vault holds tokens (the operator withdraws first), closes both vaults
with the market's own seeds, and closes the market account through
`close(dest = operator)`, so all three rents return to the operator. Tested by
`close_market_returns_all_three_rents` (rent back to the lamport, all three
accounts closed), `close_market_refuses_while_a_vault_holds_tokens` (each
vault on its own) and `close_market_rejects_non_operator`
(`QuasarError::HasOneMismatch`).

Four refusals had no test. `swap_rejects_non_positive_price` (a zero and a
negative price), `swap_rejects_oracle_scale_mismatch` (a feed at scale 6 for a
market pinned at 8), `swap_rejects_oracle_data_too_short` (a 20-byte feed
account owned by the recorded program) and
`swap_rejects_amount_that_rounds_to_zero` (a 1-minor-unit USDC buy) now
assert `NON_POSITIVE_PRICE`, `ORACLE_SCALE_MISMATCH`, `ORACLE_DATA_TOO_SHORT`
and `AMOUNT_ROUNDS_TO_ZERO`.

## Unreleased, 2026-10-05

The quasar-test suite mints NVDAx with 8 decimals, its real count, and USDC
with 6, so the two mints no longer share a decimal count (`NVDAX_DECIMALS`,
`ONE_NVDAX`, `USDC_DECIMALS`, `ONE_USDC`). The walkthrough amounts are the
same: 5 NVDAx (`FIVE_NVDAX`, now 500,000,000 minor units) costs 825.825 USDC
at the ask (`FIVE_NVDAX_AT_THE_ASK`) and sells for 824.175 at the bid
(`FIVE_NVDAX_AT_THE_BID`), 850.85 at $170 and 829.125 at a 50 bps spread, all
exact, because the ask and bid have three decimal places of a dollar and 5
is a whole number of NVDAx at any decimal count. Every refusal test asserts
its error code with `fails_with`: the program's codes for slippage,
staleness, a pre-restart price, confidence, pause, zero amounts, inventory
bounds and parameter bounds, and `QuasarError::HasOneMismatch` for the
`has_one(operator)` constraint that refuses an imposter operator. No program
source changed: the quote math reads both mints' decimals from the market and
already rounds the ask up, the bid down and both outputs down.

## 2026-10-04

Check which program owns the price feed. `initialize_market` records the
feed account's owning program on the new `Market.price_feed_program`, read
from the account's owner at that moment, beside the feed address and scale it
already pins. `read_oracle_price` now takes the feed account and that program
rather than the feed's bytes, and refuses a feed account owned by any other
with the new `PRICE_FEED_NOT_FROM_ORACLE` (14), before it decodes a byte, so
`swap` no longer accepts any account laid out like a feed as a price. Tested
by `swap_rejects_price_feed_from_another_program`, which rewrites the feed as
a byte-identical copy owned by an unrelated program and then restores the
owner.

## 2026-09-23

Documentation only: a production feed is now described as a Pyth
`PriceUpdateV2` account, since the oracle network the test feed was modeled on
has shut down.

## 2026-09-10

The `Market` account now owns both vaults and signs their outgoing transfers
with its own seeds, the way the escrow example's `offer` account does for its
vault. The separate dataless signing PDA at seeds `["authority", market]`,
its `Seeds` struct and the bump `Market` stored for it are gone, so
`initialize_market`, `swap` and `withdraw_inventory` each take one account
fewer. Asserted in `initialize_market_creates_market_and_stocked_vaults`.

## 2026-08-04

Reject oracle prices from before a cluster restart: `read_oracle_price`
requires the feed's slot to be after the `LastRestartSlot` sysvar's slot
(`PRICE_PREDATES_RESTART`). quasar-lang has no LastRestartSlot sysvar, so
`src/last_restart.rs` declares the layout and reads it via
`sol_get_sysvar`. Also pinned `zeropod = "=0.3.3"` (zeropod 0.3.4 moved to
wincode 0.5 while quasar-lang's pinned rev stays on wincode 0.4, so a fresh
resolve failed every Pod* trait bound). Tested by
`swap_rejects_price_from_before_a_restart`.

## [2026-07-22]

### Changed

- Migrated to Quasar 0.1.0 (`0.1.0-release` branch, rev `be60fca`): Quasar.toml
  rewritten to the 0.1.0 schema, `idl-build` feature and `lib` crate-type added,
  and tests rewritten from the direct QuasarSVM harness to `quasar-test`
  (`#[quasar_test]` fixtures, `crate::cpi` instruction builders, `Outcome`
  assertions). quasar-test has no `with_slot`, so the tests pin the current
  slot by writing the Clock sysvar ACCOUNT via `test.set_account` (the SVM
  fills its sysvar cache from provided accounts first), which keeps the
  stale-price scenario expressible. The `quasar-svm` git dev-dependency is
  gone. Program-source fix for 0.1.0: `Seed` is no longer in the prelude, so
  `swap.rs` and `withdraw_inventory.rs` now import it from `quasar_lang::cpi`.

## 2026-07-11 (later)

Retuned the walkthrough trade to 5 NVDAx (825.825 USDC at the ask,
824.175 back at the bid, 1.65 round-trip spread) so the numbers match the
book's convention that every character starts with 1,000 USDC. Same math,
same gates; only the amounts changed.

## 2026-07-11

Initial version: Quasar port of the prop-amm example, matching the Anchor
sibling's design and math.
