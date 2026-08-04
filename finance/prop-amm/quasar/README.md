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
  slot, confidence) and the program reads them the same way it would read a
  real Switchboard feed.
- **State writes** use Quasar's zero-copy field accessors (`field.get()` /
  `field.set()`) and `set_inner`, rather than Anchor's `Account` mutation.

## Testing

Tests run in-process with [`quasar-svm`](https://github.com/blueshift-gg/quasar-svm).
They build the program, set up both mints, an oracle feed at $165, and an
operator with funded inventory, then verify the quote math to the minor unit
in both directions, the exact 1.65 USDC round-trip spread, oracle repricing
and re-quoting, the operator's full exit, and that every gate shuts: slippage,
staleness, restart handling, confidence, pause, zero amounts, inventory bounds, and operator
access control.

```bash
quasar build
cargo test tests::
```
