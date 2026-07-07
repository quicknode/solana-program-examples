# Order-book: Kani proofs

Formal-verification harnesses for the order-book program, in the spirit of
[`aeyakovenko/percolator`](https://github.com/aeyakovenko/percolator), which
uses the [Kani](https://github.com/model-checking/kani) model checker to prove
the mathematical correctness of a DeFi engine.

## What is verified

The on-chain instructions move tokens through SPL CPIs that Kani cannot
symbolically execute, but the program's interesting logic (the price-time
matching engine, the maker-funded ceiling fee, the taker's price-improvement
rebate, and the two-lot price/quantity conversions) is pure integer
arithmetic. This crate reproduces those formulas faithfully and proves their
invariants:

- `proof_matching_conserves_quantity`: **Matching conservation**: `total_filled + taker_remaining == incoming_quantity` (and so `place_order`'s `quantity.checked_sub(taker_remaining)` never underflows).
- `proof_matching_respects_price_and_maker_size`: Every fill clears at a price that crosses the taker's limit and never exceeds the resting maker's size.
- `proof_fee_is_ceiling_and_bounded`: `fee = ⌈gross·bps/10000⌉` is a true ceiling and never exceeds `gross`, so `gross − fee` never underflows and the `require!(fee_quote <= gross_quote)` guard is unreachable dead code.
- `proof_bid_rebate_is_non_negative`: A taker bid locks at its limit but fills at the (better) maker price, so `locked − gross ≥ 0`: the price-improvement rebate never underflows.
- `proof_remaining_quantity_consistent`: `remaining + filled == original`, `remaining <= original`.

## Bounded model checking

The matching/bookkeeping proofs are pure linear logic and run at **full `u64`
width** (only the book depth is bounded, to 4 resting leaves via `unwind`). The
fee and rebate proofs verify **nonlinear 128-bit arithmetic** (`gross·bps`, and
the three-way `price·qty·lot` product), the hard case for a bit-precise solver,
so (as percolator does) they bound their symbolic inputs to a representative
range. The identities are scale-invariant, so the bounded domain still exercises
every rounding / crossing boundary.

- `proof_matching_conserves_quantity`: 4 leaves, full `u64` values, ~43s
- `proof_matching_respects_price_and_maker_size`: 4 leaves, full `u64` values, <1s
- `proof_fee_is_ceiling_and_bounded`: `gross <= 255`, `bps` fully symbolic, ~75s
- `proof_bid_rebate_is_non_negative`: prices/qty/lot `<= 31`, ~3s
- `proof_remaining_quantity_consistent`: full `u64`, <1s

These proofs run **weekly in CI** (the `kani.yml` `verify` job), not on every
push/PR, because they are slow. A fast unit-test job runs per push/PR.

## Observations

- The ceiling fee can make `fee == gross` on dust fills (e.g. `gross = 1`), so a
  maker can net zero quote on a sub-unit fill. This is intended (the comment in
  `place_order` notes ceiling rounding is in the protocol's favour to stop
  fee-dust farming), not a bug: the proof confirms `fee <= gross` always holds,
  so the maker is never *overdrawn*.

## Running

```bash
# Plain unit tests (no Kani required):
cargo test

# Formal verification (requires Kani):
cargo install --locked kani-verifier && cargo kani setup   # one-time
cargo kani
```
