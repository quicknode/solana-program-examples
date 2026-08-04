# Solana Perpetual Futures (Quasar)

A [Quasar](https://quasar-lang.com/docs) port of the Solana perpetual futures example.
The design, math, and behaviour match the Anchor implementation at
[`../anchor`](../anchor). Read that README for the full walkthrough of the
oracle-priced, pool-collateralized model, the funding mechanism, and the money
math. This page only covers what differs in the Quasar version.

## Differences from the Anchor version

- **One position per trader per pool.** The Anchor version seeds the position
  PDA by side (`[b"position", pool, owner, side]`) so a trader can hold a long
  and a short at once. Quasar's `address` constraint can only reference account
  inputs, not instruction arguments, so the side cannot be a seed; the position
  PDA is `[b"position", pool, owner]` and the side is stored in the account. A
  trader therefore holds a single open position per pool here.
- **A hand-declared `LastRestartSlot` sysvar.** quasar-lang ships only the
  Clock and Rent sysvars, so `src/last_restart.rs` declares the 8-byte layout
  itself and reads it with the same `sol_get_sysvar` syscall.
  `read_oracle_price` uses it to reject prices published before a cluster
  restart, which slot-based staleness alone cannot catch (a halt passes hours
  of wall-clock time in zero slots).
- **Oracle feed in tests.** Rather than a separate mock-oracle program, the
  tests write the feed account's bytes directly (price, scale, last-update slot)
  and the program reads them the same way it would read a real Switchboard feed.
- **State writes** use Quasar's zero-copy field accessors (`field.get()` /
  `field.set()`) and `set_inner`, rather than Anchor's `Account` mutation.

## Testing

Tests run in-process with [`quasar-svm`](https://github.com/blueshift-gg/quasar-svm).
They build the program, set up a collateral mint, oracle feed, and funded
wallets, then exercise pool initialization, liquidity add/remove, opening and
closing a long in profit, leverage rejection, liquidation, and fee collection.

```bash
cargo build-sbf
cargo test tests::
```

`cargo build-sbf` first, so the tests can load the compiled program from
`target/deploy/`.
