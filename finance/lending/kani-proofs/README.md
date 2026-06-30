# Lending — Kani proofs

Formal-verification harnesses for the lending program, in the spirit of
[`aeyakovenko/percolator`](https://github.com/aeyakovenko/percolator), which
uses the [Kani](https://github.com/model-checking/kani) model checker to prove
the mathematical correctness of a DeFi engine.

This is the richest of the finance examples — a Solend-style pool — so it gets
the most proofs.

## What is verified

The token movement is delegated to SPL CPIs Kani cannot symbolically execute,
but the money-math is pure integer arithmetic. This crate reproduces the
formulas faithfully and proves their invariants:

| Harness | Property |
| --- | --- |
| `proof_mul_div_floor_ceil_correct` | `mul_div_floor`/`mul_div_ceil` are the true floor/ceil of `a·b/d`, differ by ≤ 1, and coincide iff the division is exact. |
| `proof_rounding_is_protocol_favourable` | `ceil ≥ floor` always — debt (rounded up) is never undercounted and a supplier claim (rounded down) never overcounted, so dust can't be extracted by round-trips. |
| `proof_interest_index_monotonic` | The cumulative borrow-rate index never decreases (`accrue_interest` multiplies by a factor ≥ 1) — borrowers always owe ≥ principal. |
| `proof_utilization_in_range` | Utilization is always a valid `[0, 10000]` bps fraction (`borrowed ≤ gross`). |
| `proof_borrow_rate_within_bounds` | The kinked rate curve stays within `[min_rate, max_rate]` for every utilization, given the config ordering `min ≤ optimal ≤ max`. |
| `proof_deposit_redeem_cannot_extract` | A deposit→redeem round-trip never returns more liquidity than was put in (both legs floor) — no rounding drain of the pool. |
| `proof_liquidation_repay_bounded_by_debt` | A liquidation never repays more than the debt (close factor ≤ 100% ⇒ `max_repay ≤ debt`). |
| `proof_seize_value_includes_bonus` | Seized value always includes the bonus (`seize ≥ repay_value`) — the liquidator is never under-compensated. |

## Bounded model checking

All these harnesses verify **nonlinear 128-bit arithmetic** — and several divide
by a *symbolic* divisor (`mul_div`'s `d`, the index `scale`, the rate curve's
`full − optimal`), the single most expensive shape for a bit-precise solver.
Following percolator's practice, each bounds its symbolic inputs to a
representative range; the identities are scale-invariant, so every rounding /
crossing boundary is still exercised.

Two harnesses go further and make a normally-constant denominator a **parameter**
so the proof can use a small one:

- the interest index uses a small symbolic `scale` instead of the real
  `FIXED_POINT_SCALE = 10^18` (the monotonicity property is scale-invariant);
- the rate curve takes `full_utilization` instead of the constant `10_000`
  (dividing by a symbolic value near 10_000 is intractable; the in-bounds
  property is identical at any scale).

| Harness | Bound | Time |
| --- | --- | --- |
| `proof_mul_div_floor_ceil_correct` | `a, b, d <= 31` | ~37s |
| `proof_rounding_is_protocol_favourable` | `a, b, d <= 127` | ~29s |
| `proof_interest_index_monotonic` | `old/accrued <= 255`, `scale <= 127` | ~5s |
| `proof_utilization_in_range` | `<= 4095` | ~1s |
| `proof_borrow_rate_within_bounds` | rates `<= 255`, `full_utilization <= 32` | ~25s |
| `proof_deposit_redeem_cannot_extract` | `<= 31` | ~6s |
| `proof_liquidation_repay_bounded_by_debt` | `debt <= 4095` | <1s |
| `proof_seize_value_includes_bonus` | `repay_value <= 4095` | <1s |

These proofs run **weekly in CI** (the `kani.yml` `verify` job), not on every
push/PR, because they are slow. A fast unit-test job runs per push/PR.

## Running

```bash
# Plain unit tests (no Kani required):
cargo test

# Formal verification (requires Kani):
cargo install --locked kani-verifier && cargo kani setup   # one-time
cargo kani
```
