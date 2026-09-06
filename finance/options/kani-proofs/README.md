# Options: Kani proofs

Formal-verification harnesses for the fully collateralized options venue, in
the spirit of [`aeyakovenko/percolator`](https://github.com/aeyakovenko/percolator),
which uses the [Kani](https://github.com/model-checking/kani) model checker to
prove the mathematical correctness of a DeFi engine.

## What is verified

The onchain instructions hand token movement to the SPL token program through
CPIs that Kani cannot symbolically execute, but the arithmetic they rely on is
pure integer math, and small: every settlement amount is a product of two
integers the writer chose, the only rounding in the program is the floor in
the fee split, and the expiry window is one comparison and its complement.
This crate reproduces those formulas (mirroring `options::contract_math`) and
the handlers' custody accounting (mirroring the `underlying_locked`,
`quote_locked` and `fees_owed` counters on the `Market` account) and proves:

- `proof_exercise_moves_exactly_the_posted_terms`: for every option the program
  would accept, physical settlement hands the holder exactly the collateral
  the writer posted and hands the writer exactly the mirrored payment, both
  positive, with a call's payment equal to a put's collateral on the same
  terms. No division means no rounding gap between posted and delivered.
- `proof_premium_split_conserves_the_premium`: fee plus the writer's share is
  exactly the premium, the fee never exceeds it, the writer always receives
  something while the fee is under 100%, and the fee is the exact floor of
  `premium * fee_bps / 10_000`.
- `proof_exercise_and_reclaim_windows_partition_time`: at every instant
  exactly one of the holder (exercise) and the writer (reclaim) can claim a
  held option's collateral. Never both, never neither.
- `proof_vault_ledger_stays_consistent_across_every_lifecycle`: **the core
  custody property.** Two options of either kind are written into the shared
  vaults and each takes one of its three exits (cancel; buy then reclaim; buy,
  exercise, collect), and after every step each vault holds exactly what the
  market owes. With every option closed and the fees swept, both vaults are
  empty: no token is created or lost on any path.

## Bounded model checking

Every product in these harnesses multiplies two symbolic values, which is
nonlinear arithmetic and the worst case for a bit-precise model checker.
Following percolator's practice, the terms are bounded and the identities are
argued to be independent of the bound:

- `proof_exercise_moves_exactly_the_posted_terms`: contracts and per-contract
  amounts at most `0xFF`. An 8-bit multiplication exercises every carry
  pattern and finishes in under a minute, where 16-bit terms run for tens of
  minutes; larger terms add magnitude, not behavior. Within the bound no
  product overflows, so the overflow refusal is pinned by a unit test.
- `proof_premium_split_conserves_the_premium`: premium and fee rate each at
  most `0xFF`. The split is a 128-bit multiply followed by a 128-bit division,
  and proving a divider exact against a multiplier is the hardest shape of
  problem a SAT solver sees: a 16-bit premium against the full fee range runs
  for hours. Eight bits on each side finish in about a second, exercise the
  floor on both sides of every carry, and the fee-equals-premium edge at the
  99.99% ceiling is pinned by a unit test.
- `proof_exercise_and_reclaim_windows_partition_time`: fully symbolic; it is
  one comparison.
- `proof_vault_ledger_stays_consistent_across_every_lifecycle`: each option's
  terms at most 15, premiums and fee rates at most 255. The ledger arithmetic
  it exercises is additions and subtractions whose behavior does not depend on
  the magnitudes.

## Running

```bash
# Plain unit tests (no Kani needed), which also pin the exact numbers the
# LiteSVM tests and the book chapter use:
cargo test

# Full verification (requires cargo-kani):
cargo kani
```
