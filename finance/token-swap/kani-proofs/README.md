# Token-swap (AMM) — Kani proofs

Formal-verification harnesses for the constant-product AMM, in the spirit of
[`aeyakovenko/percolator`](https://github.com/aeyakovenko/percolator), which
uses the [Kani](https://github.com/model-checking/kani) model checker to prove
the mathematical correctness of a DeFi engine.

## What is verified

The on-chain instructions hand token movement to the SPL token program through
CPIs that Kani cannot symbolically execute, but the *interesting* part — the
constant-product curve, the fee split, the integer square root used for the
initial LP mint, and the proportional-withdraw math — is pure integer
arithmetic. This crate reproduces those formulas faithfully (same `u128`
widening, multiply-before-divide, floor rounding) and proves their invariants:

| Harness | Property |
| --- | --- |
| `proof_fee_split_bounds` | `fee <= input`, `admin_portion <= fee`, and `taxed_input + fee == input`. |
| `proof_swap_preserves_constant_product` | **The core safety property**: a swap never decreases `k = reserve_in * reserve_out`. |
| `proof_swap_cannot_fully_drain_when_reserve_positive` | With a non-empty input reserve, output is always `< other_reserve` (pool stays solvent). |
| `proof_swap_drains_pool_at_zero_reserve` | **Finding** (expected-fail, see below). |
| `proof_integer_sqrt_is_floor` | `integer_sqrt` returns the exact floor: `r² <= n < (r+1)²`. |
| `proof_withdraw_never_exceeds_reserve` | An LP can never withdraw more than the reserve holds (the `MINIMUM_LIQUIDITY` floor guarantees it). |
| `proof_deposit_clamp_never_exceeds_request` | The ratio clamp never spends more of either token than the caller offered. |

## Bounded model checking

Several harnesses verify **nonlinear 128-bit arithmetic** (e.g.
`reserve_in * reserve_out`), the worst case for a bit-precise model checker —
Kani bit-blasts the full multiplier into SAT. Following percolator's own
practice (it bounds inputs to ranges like `±500`), these harnesses constrain
their symbolic inputs to a representative range so the solver stays fast. The
identities being proven are scale-invariant, so the bounded domain still
exercises every rounding boundary. This is why these proofs are **not yet wired
into CI** — they need their bounds, whereas the escrow proofs run unbounded in
seconds.

## Finding: full drain at a zero effective reserve

`proof_swap_drains_pool_at_zero_reserve` shows the `output < other_reserve`
bound is tight: when the input-side *effective* reserve is exactly `0`, the
constant-product curve outputs the **entire** opposite reserve, draining that
side to zero. The end-of-swap `require!(new_invariant >= invariant)` guard does
**not** catch it — with `this_reserve == 0` the pre-trade product
`k = 0 * other_reserve = 0`, so the post-trade product (also `0`) trivially
satisfies `0 >= 0`.

**Severity: latent edge, not a live exploit.** Reaching `effective_reserve == 0`
on one side while the other is non-empty is a degenerate state the deposit path
is designed to prevent (the `MINIMUM_LIQUIDITY` floor keeps the bootstrap
product positive, and `proof_swap_preserves_constant_product` shows ordinary
swaps keep both sides positive). The finding shows the invariant check alone is
not sufficient for solvency — it leans on the deposit flow never letting a
reserve hit zero. A belt-and-suspenders `require!(this_reserve > 0)` in
`swap_tokens` would close it directly.

## Running

```bash
# Plain unit tests (no Kani required):
cargo test

# Formal verification (requires Kani):
cargo install --locked kani-verifier && cargo kani setup   # one-time
cargo kani
```
