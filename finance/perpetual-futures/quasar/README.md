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
  tests write the feed account's bytes directly (price, scale, last-update
  slot, confidence) as a system-owned account, and the program reads them the
  same way it would read a real oracle feed. The pool records the program that
  owns the feed account at creation on `Pool.price_feed_program` and every
  read refuses a feed account owned by any other program with
  `PRICE_FEED_NOT_FROM_ORACLE` (23); here that recorded program is the system
  program, and `open_rejects_price_feed_from_another_program` rewrites the
  feed with another owner to show the refusal is on the owner alone.
- **State writes** use Quasar's zero-copy field accessors (`field.get()` /
  `field.set()`) and `set_inner`, rather than Anchor's `Account` mutation.

## Testing

Tests run in-process with [`quasar-svm`](https://github.com/blueshift-gg/quasar-svm).
They build the program, set up a collateral mint, oracle feed, and funded
wallets, then exercise:

- pool initialization, including its checks on the initial margin (above the
  maintenance margin, at most 10,000 basis points), the close fee (below the
  maintenance margin) and the price band (above zero, below 10,000 basis
  points)
- liquidity add/remove, the first deposit on both sides of the withheld
  minimum (`first_deposit_below_minimum_fails`), and share inflation through a
  provider's own trades
- opening and closing a long in profit, and the initial margin on both sides
  of its boundary
- stale-price, pre-restart-price, and wide-confidence rejection, and a feed
  account owned by another program refused on the owner alone
  (`open_rejects_price_feed_from_another_program`)
- the funding-rate maximum, an operator's wallet on the lighter side earning
  only the fixed rate, and the exact funding a long pays over its time open,
  which follows seconds rather than slots (`funding_follows_seconds_not_slots`)
- the price band: opens, closes, deposits and withdrawals refused when the
  oracle jumps outside it, liquidation running outside it, the exact average
  after one `update_price_average`
  (`single_update_moves_average_by_elapsed_fraction`), and repeated updates
  walking the average to a genuine move until trading resumes
  (`price_average_catches_up_after_genuine_move`), and one manipulated read
  after an idle window leaving the average where it was
  (`one_manipulated_read_after_idle_does_not_move_average`)
- liquidation, and fee collection
- every fee and the maintenance requirement rounding up
  (`fees_and_maintenance_requirement_round_up`), and `basis_points_of` at
  its boundaries
- the haircut: a position opening without full backing, profit paid in full
  while the pool backs it, two winners each paid exactly half when the pool is
  stressed (`haircut_scales_profit_when_pool_stressed`), the insurance fund
  paying a profit beyond `liquidity`, and a winner offset by an open loser paid
  the pool's whole backing rather than refused
  (`winner_offset_by_open_loser_is_paid_not_refused`)
- the profit warm-up on both sides of its boundary
  (`profit_blocked_before_maturation`, `profit_realized_after_maturation`),
  and a loss closing in the slot it opened
- the insurance fund: its exact share of each fee, a bankrupt position's
  deficit paid by the fund, and a bankrupt position liquidated for no fee with
  the fund paying before the providers
  (`liquidation_of_bankrupt_position_charges_insurance_before_liquidity`)
- withdrawals capped at `liquidity` while traders are down
  (`remove_liquidity_capped_at_liquidity`)

Program errors are `ProgramError::Custom` codes listed in
`instructions/shared.rs`, with the same names as the Anchor version's
`PerpError` variants in upper snake case: `INITIAL_MARGIN_NOT_MET` (2),
`INITIAL_MARGIN_NOT_ABOVE_MAINTENANCE` (19), `INVALID_PRICE_DEVIATION` (20),
`PRICE_OUTSIDE_BAND` (21), `PROFIT_NOT_MATURED` (22) and
`PRICE_FEED_NOT_FROM_ORACLE` (23) among them.
`update_price_average` is discriminator 7. `initialize_pool` takes the Anchor
version's `PoolParameters` fields as separate arguments, ending with
`insurance_fee_bps` and `profit_warmup_slots`.

```bash
cargo build-sbf
cargo test tests::
```

`cargo build-sbf` first, so the tests can load the compiled program from
`target/deploy/`.
