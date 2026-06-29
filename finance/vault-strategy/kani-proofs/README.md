# Vault-strategy — Kani proofs

Formal-verification harnesses for the ERC4626-style share vault, in the spirit
of [`aeyakovenko/percolator`](https://github.com/aeyakovenko/percolator), which
uses the [Kani](https://github.com/model-checking/kani) model checker to prove
the mathematical correctness of a DeFi engine.

## What is verified

Depositors mint share tokens against the vault's net asset value; withdrawals
burn shares for a proportional slice of every vault balance; a manager fee mints
a small slice of shares over time. Token movement is via SPL CPIs Kani cannot
symbolically execute, but the share math is pure integer arithmetic:

| Harness | Property |
| --- | --- |
| `proof_withdraw_within_balance` | **Solvency**: a withdrawal never takes more of any vault balance than it holds (`floor(balance·shares/total) <= balance`, since `shares <= total`); burning the whole supply takes exactly the whole balance. |
| `proof_deposit_withdraw_cannot_extract` | A deposit→withdraw round-trip never returns more than was deposited — no rounding attack mints shares worth more than they cost. |
| `proof_fee_shares_bounded_by_supply` | The time-based manager fee can never mint more than 100%/year of dilution (`fee_shares <= total_shares` for `elapsed <= 1yr`, `fee_bps <= 10000`). |

## Bounded model checking

All three verify nonlinear 128-bit arithmetic with a symbolic divisor (the share
supply / NAV), so — as percolator does — they bound their symbolic inputs to a
representative range; the share identities are scale-invariant.

| Harness | Bound | Time |
| --- | --- | --- |
| `proof_withdraw_within_balance` | balances/supply `<= 255` | ~12s |
| `proof_deposit_withdraw_cannot_extract` | `<= 31` | ~3s |
| `proof_fee_shares_bounded_by_supply` | `<= 255` | ~4s |

Not wired into CI (bounded); the fast, unbounded escrow proofs gate CI.

## Running

```bash
cargo test                                                 # unit tests, no Kani
cargo install --locked kani-verifier && cargo kani setup   # one-time
cargo kani                                                  # formal verification
```
