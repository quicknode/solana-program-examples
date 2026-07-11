# Prop AMM: Kani proofs

Formal-verification harnesses for the oracle-quoted prop AMM, in the spirit of
[`aeyakovenko/percolator`](https://github.com/aeyakovenko/percolator), which
uses the [Kani](https://github.com/model-checking/kani) model checker to prove
the mathematical correctness of a DeFi engine.

## What is verified

The on-chain instructions hand token movement to the SPL token program through
CPIs that Kani cannot symbolically execute, but the *interesting* part — the
ask/bid construction, the amount conversion across oracle scale and token
decimals, and the "never pay out more than oracle value" invariant — is pure
integer arithmetic. This crate reproduces those formulas faithfully (same
`u128` widening, multiply-before-divide, ask ceiled, bid and outputs floored;
mirrors `prop_amm::quote_math`) and proves the invariants:

- `proof_quote_brackets_oracle`: `bid <= oracle <= ask` for every valid price
  and spread, and both roundings are *exact* (the ask is the smallest integer
  at or above the true ratio, the bid the largest at or below), so both
  under-rounding against the market and over-rounding against the trader fail
  the proof.
- `proof_buy_never_exceeds_oracle_value`: **The core safety property**, buy
  side: the base handed out is never worth more at the raw oracle price than
  the quote taken in. This is exactly the swap handler's post-math
  `InvariantViolated` assert — the proof says it can never fire while the
  quoting math is intact.
- `proof_sell_never_exceeds_oracle_value`: the same property, sell side.
- `proof_round_trip_never_profits_the_trader`: buying and immediately selling
  back returns no more quote than went in, for every price, spread, and
  decimal configuration — a trader cannot mint money by bouncing off both
  sides of the quote.

## Bounded model checking

The output-amount harnesses divide by a *symbolic* ask or bid — symbolic ÷
symbolic 128-bit division, the hardest case for a bit-precise model checker.
Following percolator's practice, amounts and prices are bounded and the
decimal exponents are kept small; the identities are independent of the
exponents' actual values (they enter both sides of each comparison
symmetrically), so the bounded domain exercises the same rounding edges as
scale 8 with 6-decimal tokens. Spreads stay fully symbolic over their entire
valid range (`1..10_000`).

- `proof_quote_brackets_oracle`: price fully symbolic (linear arithmetic)
- `proof_buy_never_exceeds_oracle_value` / `proof_sell_never_exceeds_oracle_value`: amounts and price `<= 1023`
- `proof_round_trip_never_profits_the_trader`: amounts and price `<= 255` (two chained symbolic divisions)

## Running

```bash
# Plain unit tests (no Kani needed) — also pin the exact numbers the LiteSVM
# tests and the book chapter use:
cargo test

# Full verification (requires cargo-kani):
cargo kani
```
