# Fundraiser: Kani model checks

Kani harnesses for the fundraiser program, in the spirit of
[`aeyakovenko/percolator`](https://github.com/aeyakovenko/percolator), which
uses the [Kani](https://github.com/model-checking/kani) model checker to check
the arithmetic of a DeFi engine. Kani marks a harness with `#[kani::proof]`,
which is why the crate is `kani-proofs` and the harnesses are named `proof_*`;
each one is a model check: Kani tries every value of the inputs the harness
declares and reports either that every assertion held or the input that breaks
one.

## What is checked

The program collects contributions toward a goal; if the goal is not met by the
deadline, every contributor reclaims their exact stake. Token movement is via
SPL CPIs Kani cannot symbolically execute, but the accounting (`contribute`,
`refund`, `close_contributor`) is pure integer arithmetic, and the harnesses check it for every input in the
declared ranges:

- `proof_open_contributor_accounts_counts_open_accounts`: `open_contributor_accounts` always equals the number of contributor accounts that exist, over every sequence of eight contributions, refunds and closes among three contributors. `close_fundraiser` requires it to be zero, so no contributor account carries over into the next raise at the same address.
- `proof_current_amount_is_sum_of_contributions`: `current_amount` always equals the sum of the contributions added to it, no accounting drift.
- `proof_refunds_sum_to_current_amount`: On a failed raise, refunds sum back to `current_amount`; no contributor reclaims more than they put in.

The counter harness explores every sequence of steps up to its bound; the two
accounting/refund harnesses are pure linear logic and run at full `u64` width
(bounded only in the number of contributors).

Run weekly in CI (the `kani.yml` `verify` job), not on every push/PR, because
model checks are slow. A fast unit-test job runs per push/PR.

## Running

```bash
cargo test                                                 # unit tests, no Kani
cargo install --locked kani-verifier && cargo kani setup   # one-time
cargo kani                                                  # model check
```
