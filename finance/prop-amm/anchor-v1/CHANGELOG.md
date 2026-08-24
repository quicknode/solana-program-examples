# Changelog

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
spread; anyone can swap against the quotes. Includes the `mock-switchboard`
oracle program for deterministic tests.
