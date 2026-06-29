# Betting-market — Kani proofs

Formal-verification harnesses for the pari-mutuel betting market, in the spirit
of [`aeyakovenko/percolator`](https://github.com/aeyakovenko/percolator), which
uses the [Kani](https://github.com/model-checking/kani) model checker to prove
the mathematical correctness of a DeFi engine.

## What is verified

Every stake lands in one vault; at settlement the losing pool (minus a fee) is
split among the winners in proportion to their stake. The token movement goes
through SPL CPIs Kani cannot symbolically execute, but the payout math is pure
integer arithmetic. This crate reproduces it faithfully and proves:

| Harness | Property |
| --- | --- |
| `proof_settlement_fee_and_split` | `fee <= losing_pool` (so `distributable` never underflows) and `winning + distributable + fee == total` — settlement conserves the pool. |
| `proof_winner_never_below_stake` | `payout = stake + winnings >= stake`: a winner is never paid less than they staked (the fee is charged only on losers). |
| `proof_parimutuel_solvency` | **Solvency** (centrepiece): the winners collectively never claim more than the vault holds after the fee (`Σ payout_i <= winning_pool + distributable`). |
| `proof_refund_conserves_pool` | On cancellation, refunds sum back to the total pool — neither over- nor under-drained. |

### The solvency proof

After settlement the vault holds `winning_pool + distributable_losing_pool`.
Each winner is paid `stake_i + floor(stake_i · D / winning_pool)`, and the
winning stakes sum to `winning_pool`. Because
`Σ floor(stake_i·D/W) <= Σ stake_i·D/W = D`, total payouts are
`<= winning_pool + D` — exactly the vault balance. So no set of winners can drain
the vault below zero; floor rounding only ever leaves dust behind. Modelled with
3 winners whose stakes sum to the winning pool.

## Bounded model checking

The settlement, payout, and solvency proofs verify nonlinear 128-bit arithmetic
(`stake · distributable`, divided by the symbolic winning pool), the hard case
for a bit-precise solver, so — as percolator does — they bound their symbolic
inputs to a representative range; the pro-rata identity is scale-invariant. The
refund proof is pure linear logic and runs at full `u64` width (bounded only in
the number of bettors).

| Harness | Bound | Time |
| --- | --- | --- |
| `proof_settlement_fee_and_split` | `total_pool <= 4095`, `fee_bps` symbolic | ~1s |
| `proof_winner_never_below_stake` | `winning_pool/distributable <= 255` | ~6s |
| `proof_parimutuel_solvency` | 3 winners, stakes `<= 7`, `distributable <= 63` | ~3s |
| `proof_refund_conserves_pool` | 4 bettors, full `u64` | <1s |

Not wired into CI (bounded); the fast, unbounded escrow proofs gate CI.

## Running

```bash
cargo test                                                 # unit tests, no Kani
cargo install --locked kani-verifier && cargo kani setup   # one-time
cargo kani                                                  # formal verification
```
