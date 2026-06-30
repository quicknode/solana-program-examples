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
| `proof_swap_at_zero_reserve_drains_whole_pool` | **Finding**, proven as a positive characterization (see below). |
| `proof_integer_sqrt_is_floor` | `integer_sqrt` returns the exact floor: `r² <= n < (r+1)²`. |
| `proof_withdraw_never_exceeds_reserve` | An LP can never withdraw more than the reserve holds (the `MINIMUM_LIQUIDITY` floor guarantees it). |
| `proof_deposit_clamp_never_exceeds_request` | The ratio clamp never spends more of either token than the caller offered. |

## Bounded model checking

Several harnesses verify **nonlinear 128-bit arithmetic** (e.g.
`reserve_in * reserve_out`, and worst of all `amount * pool_b / pool_a` where
the *divisor* is symbolic), the hardest case for a bit-precise model checker —
Kani bit-blasts the full multiplier/divider into SAT. Following percolator's own
practice (it bounds inputs to ranges like `±500`), these harnesses constrain
their symbolic inputs to a representative range so the solver stays fast. The
identities being proven are scale-invariant, so the bounded domain still
exercises every rounding boundary. The bound is per-harness, sized to its
difficulty:

| Harness | Input bound | Time |
| --- | --- | --- |
| `proof_fee_split_bounds` | `input <= 4095`, fractions fully symbolic | ~2s |
| `proof_swap_preserves_constant_product` | reserves/input `<= 63` | ~26s |
| `proof_swap_cannot_fully_drain_when_reserve_positive` | reserves/input `<= 255` | ~7s |
| `proof_swap_at_zero_reserve_drains_whole_pool` | `<= 255` | ~15s |
| `proof_integer_sqrt_is_floor` | `n <= 255`, `unwind(11)` | ~33s |
| `proof_withdraw_never_exceeds_reserve` | `<= 4095` | ~5s |
| `proof_deposit_clamp_never_exceeds_request` | `<= 31` (symbolic divisor) | ~3s |

The whole suite verifies in ~90s of solver time. This is why these proofs run
**weekly in CI** (the `kani.yml` `verify` job), not on every push/PR. A fast
unit-test job runs per push/PR.

## Finding: full drain at a zero effective reserve

`proof_swap_at_zero_reserve_drains_whole_pool` shows the `output < other_reserve`
bound is tight: when the input-side *effective* reserve is exactly `0`, the
constant-product curve outputs the **entire** opposite reserve (`output ==
other_reserve`), draining that side to zero. The end-of-swap
`require!(new_invariant >= invariant)` guard does **not** catch it — with
`this_reserve == 0` the pre-trade product `k = 0 * other_reserve = 0`, so the
post-trade product (also `0`) trivially satisfies `0 >= 0`.

It is encoded as a **positive** proof (every assertion holds — `output ==
other_reserve` and `0 >= 0`), not a `#[kani::should_panic]`. A should-panic would
invert the maintenance signal: adding the `require!(this_reserve > 0)` fix would
make a should-panic harness start failing.

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
