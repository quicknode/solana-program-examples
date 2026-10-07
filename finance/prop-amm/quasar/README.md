# Solana Prop AMM (Quasar)

A [Quasar](https://quasar-lang.com/docs) port of the Solana prop-amm example. The
design, math, and behaviour match the Anchor implementation at
[`../anchor`](../anchor). Read that README for the full walkthrough of the
oracle-quoted, operator-owned model. This page only covers what differs in the
Quasar version.

## Differences from the Anchor version

- **`Direction` is a `u8`.** Quasar instruction arguments are plain integers,
  so the Anchor sibling's `Direction` enum becomes `0` (buy base at the ask)
  or `1` (sell base at the bid), with named constants in `constants.rs`.
- **`paused` is a `u8`.** The account layout is zero-copy, so the flag is
  `0`/`1` rather than a `bool`.
- **Trader token accounts must already exist.** The Anchor version uses
  `init_if_needed` to create the trader's destination account inside the swap;
  here the tests create both token accounts up front.
- **A hand-declared `LastRestartSlot` sysvar.** quasar-lang ships only the
  Clock and Rent sysvars, so `src/last_restart.rs` declares the 8-byte layout
  itself and reads it with the same `sol_get_sysvar` syscall.
  `read_oracle_price` uses it to reject prices published before a cluster
  restart, which slot-based staleness alone cannot catch (a halt passes hours
  of wall-clock time in zero slots).
- **Oracle feed in tests.** Rather than a separate mock-oracle program, the
  tests write the feed account's bytes directly (price, scale, last-update
  slot, confidence) as a system-owned account, and the program reads them the
  same way it would read a real oracle feed. The market records the program
  that owns the feed account at creation on `Market.price_feed_program` and
  every read refuses a feed account owned by any other program with
  `PRICE_FEED_NOT_FROM_ORACLE` (14); here that recorded program is the system
  program, and `swap_rejects_price_feed_from_another_program` rewrites the
  feed with another owner to show the refusal is on the owner alone.
- **State writes** use Quasar's zero-copy field accessors (`field.get()` /
  `field.set()`) and `set_inner`, rather than Anchor's `Account` mutation.

## Testing

Tests run in-process with [`quasar-svm`](https://github.com/blueshift-gg/quasar-svm).
They build the program, set up both mints (NVDAx with 8 decimals, USDC with
6), an oracle feed at $165, and an operator with funded inventory, then verify
the quote math to the minor unit in both directions, the exact 1.65 USDC
round-trip spread, oracle repricing and re-quoting, the operator's full exit,
and that every gate shuts: slippage, staleness, restart handling, confidence,
a feed account owned by another program, pause, zero amounts, inventory
bounds, parameter bounds, and operator access control. The oracle reader's
layout and value checks each have a test: a zero or negative price
(`swap_rejects_non_positive_price`), a feed at another scale
(`swap_rejects_oracle_scale_mismatch`), a feed account too short to decode
(`swap_rejects_oracle_data_too_short`), and a buy too small to deliver one
minor unit (`swap_rejects_amount_that_rounds_to_zero`). `close_market`
(discriminator 5) closes both vaults and the market and returns the three
rents to the operator, refusing with `INVENTORY_NOT_EMPTY` (15) while either
vault holds tokens; `close_market_returns_all_three_rents`,
`close_market_refuses_while_a_vault_holds_tokens` and
`close_market_rejects_non_operator` test it. Every refusal test
asserts its error code with `fails_with`: the program's own codes from
`instructions::shared::error`, and `QuasarError::HasOneMismatch` for the
`has_one(operator)` constraint that refuses an imposter operator.

```bash
quasar build
cargo test tests::
```
