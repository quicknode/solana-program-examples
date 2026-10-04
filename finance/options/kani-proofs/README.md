# Options: Kani model checks

Kani harnesses for the fully collateralized options venue, in the spirit of
[`aeyakovenko/percolator`](https://github.com/aeyakovenko/percolator), which
uses the [Kani](https://github.com/model-checking/kani) model checker to check
the arithmetic of a DeFi engine. Kani marks a harness with `#[kani::proof]`,
which is why the crate is `kani-proofs` and the harnesses are named `proof_*`;
each one is a model check: Kani tries every value of the inputs the harness
declares and reports either that every assertion held or the input that breaks
one.

## What is checked

The onchain instructions hand token movement to the SPL token program through
CPIs that Kani cannot symbolically execute, but the arithmetic they rely on is
pure integer math, and small: settlement moves the two amounts the writer
chose and the option stores, the only rounding in the program is the floor in
the fee split, and the expiry window is one comparison and its complement.
This crate reproduces those formulas (mirroring `options::contract_math`) and
the handlers' custody accounting (mirroring the `underlying_owed`,
`quote_owed` and `fees_owed` counters on the `Market` account) and checks, for every input
in the declared ranges:

- `proof_exercise_moves_exactly_the_posted_terms`: for every option the program
  would accept, physical settlement hands the holder exactly the collateral
  the writer posted and hands the writer exactly the mirrored payment, both
  positive, with a call's payment equal to a put's collateral on the same
  terms. Settlement does no arithmetic, so nothing can open a gap between
  posted and delivered.
- `proof_premium_split_conserves_the_premium`: fee plus the writer's share is
  exactly the premium, the fee never exceeds it, the writer always receives
  something while the fee is under 100%, and the fee is the exact floor of
  `premium * fee_bps / 10_000`.
- `proof_exercise_and_reclaim_windows_partition_time`: at every instant
  exactly one of the holder (exercise) and the writer (reclaim) can claim a
  held option's collateral. Never both, never neither.
- `proof_vault_ledger_stays_consistent_across_every_lifecycle`: **the core
  custody invariant.** Two options of either kind are written into the shared
  vaults and each takes one of its three exits (cancel; buy then reclaim; buy,
  exercise, collect), and after every step each vault holds exactly what the
  market owes. With every option closed and the fees swept, both vaults are
  empty: no token is created or lost on any path.
- `proof_collect_fees_pays_only_the_fees_owed`: the admin reaches only the
  fees. From any ledger whose quote vault covers what it owes, plus any
  surplus sent straight to the vault, one `collect_fees` pays the admin exactly
  `fees_owed`, leaves every amount owed to writers and holders and the surplus
  in the vault, zeroes `fees_owed`, and a second sweep pays nothing.

## Bounded model checking

A product of two symbolic values is nonlinear arithmetic and the worst case
for a bit-precise model checker. Following percolator's practice, harnesses
that multiply bound their terms and argue the identities are independent of
the bound:

- `proof_exercise_moves_exactly_the_posted_terms`: fully symbolic; the
  amounts are any nonzero u64, since settlement does no arithmetic.
- `proof_premium_split_conserves_the_premium`: premium and fee rate each at
  most `0xFF`. The split is a 128-bit multiply followed by a 128-bit division,
  and checking a divider exact against a multiplier is the hardest shape of
  problem a SAT solver sees: a 16-bit premium against the full fee range runs
  for hours. Eight bits on each side finish in about a second, exercise the
  floor on both sides of every carry, and the fee-equals-premium edge at the
  99.99% ceiling is pinned by a unit test.
- `proof_exercise_and_reclaim_windows_partition_time`: fully symbolic; it is
  one comparison.
- `proof_vault_ledger_stays_consistent_across_every_lifecycle`: each option's
  amounts, premiums and fee rates at most 255. The ledger arithmetic
  it exercises is additions and subtractions whose behavior does not depend on
  the magnitudes.
- `proof_collect_fees_pays_only_the_fees_owed`: fully symbolic; the owed
  amounts, the fees and the surplus are any u64 whose sum fits in a vault
  balance, since the sweep is one subtraction.

## Running

```bash
# Plain unit tests (no Kani needed), which pin the exact numbers the LiteSVM
# tests and the book chapter use and run the collect_fees check on them:
cargo test

# Full model check (requires cargo-kani):
cargo kani
```
