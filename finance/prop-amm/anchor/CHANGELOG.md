# Changelog

## Unreleased, 2026-10-05

The LiteSVM suite mints NVDAx with 8 decimals, its real count, and USDC with
6, so the two mints no longer share a decimal count (`NVDAX_DECIMALS`,
`ONE_NVDAX`, `USDC_DECIMALS`, `ONE_USDC`). The walkthrough amounts are the
same: 5 NVDAx (`FIVE_NVDAX`, now 500,000,000 minor units) costs 825.825 USDC
at the ask (`FIVE_NVDAX_AT_THE_ASK`) and sells for 824.175 at the bid
(`FIVE_NVDAX_AT_THE_BID`), 850.85 at $170 and 829.125 at a 50 bps spread, all
exact, because the ask and bid have three decimal places of a dollar and 5
is a whole number of NVDAx at any decimal count. Every refusal test asserts
its error code: `assert_fails_with` for the program's errors (slippage,
staleness, a pre-restart price, confidence, pause, zero amounts, inventory
bounds and parameter bounds) and the new `assert_fails_with_anchor_error`
for the `address = market.operator` constraint (`ConstraintAddress`) that refuses an imposter
operator; `move_inventory_as` and `set_quote_as` return the transaction error
text for it. No program source changed: the quote math reads both mints'
decimals from the market and already rounds the ask up, the bid down and
both outputs down.

## 2026-10-04

Check which program owns the price feed. `initialize_market` records the
feed account's owning program on the new `Market.price_feed_program`, read
from the account's owner at that moment, beside the feed address and scale it
already pins. `read_oracle_price` takes that program and refuses a feed
account owned by any other with the new `PriceFeedNotFromOracle`, before it
decodes a byte, so `swap` no longer accepts any account laid out like a feed
as a price. Tested by `test_swap_rejects_price_feed_from_another_program`,
which swaps the feed for a byte-identical copy owned by an unrelated program
and then restores the owner. The `errors` module is public so the tests can
match `PropAmmError` codes, and the suite's `swap` and `try_new` return the
transaction error text.

## 2026-09-23

The mock oracle program is now `mock-price-feed` (library and program
`mock_price_feed`, package `mock_price_feed_prop_amm`), with the same program ID, instructions and
account layout. The oracle network it was modeled on has shut down, so the
production path described in `state/oracle.rs` now reads a Pyth
`PriceUpdateV2` account, as `basics/pyth` does. No behavior changes.

## 2026-09-10

The `Market` account now owns both vaults and signs their outgoing transfers
with its own seeds, the way the escrow example's `offer` account does for its
vault. The separate dataless signing PDA at seeds `["authority", market]`,
the bump `Market` stored for it and its seed constant are gone, so
`initialize_market`, `swap` and `withdraw_inventory` each take one account
fewer. Tested by `test_market_owns_both_vaults`.

## 2026-08-04

Reject oracle prices from before a cluster restart. A halt stops the slot
count but not the wall clock, so after a restart a feed can look fresh in
slots while its price is hours old; for a market maker that is a free option
for whoever trades first. `read_oracle_price` now also requires the feed's
slot to be after the `LastRestartSlot` sysvar's slot
(`PricePredatesRestart`). Tested by
`test_swap_rejects_price_from_before_a_restart`.

## 2026-07-11 (later)

Retuned the walkthrough trade to 5 NVDAx (825.825 USDC at the ask,
824.175 back at the bid, 1.65 round-trip spread) so the numbers match the
book's convention that every character starts with 1,000 USDC. Same math,
same gates; only the amounts changed.

## 2026-07-11

Initial version: an oracle-quoted proprietary AMM. One operator funds the
market's inventory and quotes both sides of it at the oracle price plus a
spread; anyone can swap against the quotes. Includes the `mock-price-feed`
oracle program for deterministic tests.
