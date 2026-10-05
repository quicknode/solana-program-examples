# Lending: Kani model checks

Kani model-check harnesses for the lending program, in the spirit of
[`aeyakovenko/percolator`](https://github.com/aeyakovenko/percolator), which
uses the [Kani](https://github.com/model-checking/kani) model checker to check
the arithmetic of a DeFi engine.

Kani marks a harness with `#[kani::proof]`, which is why the crate is
`kani-proofs` and the harnesses are named `proof_*`; each one is a model check:
it tries every value of its declared inputs and reports either that every
assertion held or the input that breaks one.

This is the richest of the finance examples (a Solend-style pool) so it gets
the most harnesses.

## What is checked

The token movement is delegated to SPL CPIs Kani cannot symbolically execute,
but everything else is pure integer arithmetic. This crate reproduces the
formulas faithfully and checks their invariants:

- `proof_mul_div_floor_ceil_correct`: `mul_div_floor`/`mul_div_ceil` are the true floor/ceil of `a·b/d`, differ by ≤ 1, and coincide iff the division is exact.
- `proof_rounding_is_program_favourable`: `ceil ≥ floor` always, debt (rounded up) is never undercounted and a supplier claim (rounded down) never overcounted, so dust can't be extracted by round-trips.
- `proof_accumulation_factor_monotonic`: The borrow accumulation factor never decreases (`accrue_interest` multiplies by a factor ≥ 1), borrowers always owe ≥ principal.
- `proof_program_fee_rounds_up_within_interest`: The program's cut of an accrual, `ceil(interest · reserve_factor / 10000)`, rounds up yet never exceeds the interest, so the suppliers' remainder never underflows and fee + remainder = interest.
- `proof_utilization_in_range`: Utilization is always a valid `[0, 10000]` bps fraction (`borrowed ≤ gross`).
- `proof_borrow_rate_within_bounds`: The kinked rate curve stays within `[min_rate, max_rate]` for every utilization, given the config ordering `min ≤ optimal ≤ max`.
- `proof_deposit_redeem_cannot_extract`: A deposit→redeem round-trip never returns more liquidity than was put in (both legs floor), no rounding drain of the pool.
- `proof_liquidation_repay_bounded_by_debt`: A liquidation never repays more than the debt (close factor ≤ 100% ⇒ `max_repay ≤ debt`).
- `proof_seize_value_includes_bonus`: Seized value always includes the bonus (`seize ≥ repay_value`), the liquidator is never under-compensated.

## Bounded model checking

All these harnesses check **nonlinear 128-bit arithmetic**, and several divide
by a *symbolic* divisor (`mul_div`'s `d`, the index `scale`, the rate curve's
`full − optimal`), the single most expensive shape for a bit-precise solver.
Following percolator's practice, each bounds its symbolic inputs to a
representative range; the identities are scale-invariant, so every rounding /
crossing boundary is still exercised.

Two harnesses go further and make a normally-constant denominator a **parameter**
so the harness can use a small one:

- the accumulation factor uses a small symbolic `scale` instead of the real
  `FIXED_POINT_SCALE = 10^18` (the monotonicity property is scale-invariant);
- the rate curve takes `full_utilization` instead of the constant `10_000`
  (dividing by a symbolic value near 10_000 is intractable; the in-bounds
  property is identical at any scale).

- `proof_mul_div_floor_ceil_correct`: `a, b, d <= 31`, ~37s
- `proof_rounding_is_program_favourable`: `a, b, d <= 127`, ~29s
- `proof_accumulation_factor_monotonic`: `old/accrued <= 255`, `scale <= 127`, ~5s
- `proof_program_fee_rounds_up_within_interest`: `interest <= 4095`, <1s
- `proof_utilization_in_range`: `<= 4095`, ~1s
- `proof_borrow_rate_within_bounds`: rates `<= 255`, `full_utilization <= 32`, ~25s
- `proof_deposit_redeem_cannot_extract`: `<= 31`, ~6s
- `proof_liquidation_repay_bounded_by_debt`: `debt <= 4095`, <1s
- `proof_seize_value_includes_bonus`: `repay_value <= 4095`, <1s

These model checks run **weekly in CI** (the `kani.yml` `verify` job), not on
every push/PR, because they are slow. A fast unit-test job runs per push/PR.

## Running

```bash
# Plain unit tests (no Kani required):
cargo test

# Kani model checks (requires Kani):
cargo install --locked kani-verifier && cargo kani setup   # one-time
cargo kani
```
