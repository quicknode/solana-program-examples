# Token-fundraiser: Kani proofs

Formal-verification harnesses for the token-fundraiser program, in the spirit of
[`aeyakovenko/percolator`](https://github.com/aeyakovenko/percolator), which
uses the [Kani](https://github.com/model-checking/kani) model checker to prove
the mathematical correctness of a DeFi engine.

## What is verified

The program collects contributions toward a goal; if the goal is not met by the
deadline, every contributor reclaims their exact stake. Token movement is via
SPL CPIs Kani cannot symbolically execute, but the accounting (`contribute`,
`refund`) is pure integer arithmetic:

- `proof_contribution_cap_bounds`: The per-contributor cap never exceeds the goal, and the `cumulative <= cap` check keeps every contributor at or below it (and below the goal).
- `proof_current_amount_is_sum_of_contributions`: `current_amount` always equals the sum of the contributions added to it, no accounting drift.
- `proof_refunds_sum_to_current_amount`: On a failed raise, refunds sum back to `current_amount`; no contributor reclaims more than they put in.

The cap proof verifies nonlinear arithmetic (`goal · pct / scaler`) and uses
bounded model checking; the two accounting/refund proofs are pure linear logic
and run at full `u64` width (bounded only in the number of contributors). The
whole suite verifies in under a second.

Run weekly in CI (the `kani.yml` `verify` job), not on every push/PR, because
the nonlinear proofs are slow. A fast unit-test job runs per push/PR.

## Running

```bash
cargo test                                                 # unit tests, no Kani
cargo install --locked kani-verifier && cargo kani setup   # one-time
cargo kani                                                  # formal verification
```
