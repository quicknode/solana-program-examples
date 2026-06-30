# Escrow — Kani proofs

Formal-verification harnesses for the escrow program, in the spirit of
[`aeyakovenko/percolator`](https://github.com/aeyakovenko/percolator), which
uses the [Kani](https://github.com/model-checking/kani) model checker to prove
the mathematical correctness of a DeFi engine.

## What is verified

The escrow program itself does almost no arithmetic — it delegates token
movement to the SPL token program through CPIs, which Kani cannot symbolically
execute. So (exactly like percolator, which verifies a self-contained library)
this crate models the escrow's *verifiable core* as pure Rust functions that
mirror the on-chain code's arithmetic and statement ordering, and proves the
invariants the program relies on:

| Harness | Property |
| --- | --- |
| `proof_token_transfer_conserves` | An SPL transfer either fails atomically or conserves the two accounts' total balance. |
| `proof_close_offer_conserves_on_success` | Closing the offer account conserves lamports and empties the source. |
| `proof_close_offer_conserves_lamports_unconditionally` | **Finding (now fixed)**: lamport conservation holds with equality on every path (see below). |
| `proof_take_offer_conserves_value` | A take conserves total mint A and total mint B, drains the vault, and pays the maker exactly the price. |
| `proof_take_offer_guard_never_overflows` | The `checked_add` conservation guards in `take_offer` are unreachable dead code. |
| `proof_take_offer_guard_dead_under_spl_invariant` | Same, shown explicitly under the SPL supply invariant. |
| `proof_cancel_offer_returns_all_to_maker` | Cancelling returns every vault token to the maker and conserves mint A. |
| `proof_make_offer_vault_equals_offered` | After a deposit the vault holds exactly the offered amount. |
| `proof_offer_id_le_bytes_roundtrip` / `proof_offer_id_seed_injective` | The PDA `id` seed encoding is a lossless, injective round-trip. |

## Finding (now fixed): lamport ordering in `close_offer_account`

`utils::close_offer_account` originally zeroed the offer account's lamports
*before* the fallible `checked_add` that credits the destination:

```rust
**offer_info.lamports.borrow_mut() = 0;                     // (1) zero source
**destination.lamports.borrow_mut() = destination_lamports  // (2) credit dest (may Err)
    .checked_add(offer_lamports)
    .ok_or(EscrowError::ArithmeticOverflow)?;
```

At `offer == dest == u64::MAX` the credit overflows and returns `Err` after the
source was already zeroed, so the total *transiently* dropped from `2·MAX` to
`MAX` — lamports momentarily destroyed on the error path. Not exploitable (the
runtime reverts on `Err`, and a wallet can't hold near `u64::MAX` lamports), but
conservation held only because of those *external* guarantees.

**The fix (applied):** `close_offer_account` now uses *compute-then-commit* —
the `checked_add` runs **before** any account is mutated, so the error path
changes nothing:

```rust
let new_destination_lamports = destination_lamports
    .checked_add(offer_lamports)
    .ok_or(EscrowError::ArithmeticOverflow)?;   // fallible first, no mutation yet
**destination.lamports.borrow_mut() = new_destination_lamports;
**offer_info.lamports.borrow_mut() = 0;
```

`proof_close_offer_conserves_lamports_unconditionally` now proves lamport
conservation holds with **equality on every path**, with no precondition — the
invariant no longer depends on the runtime reverting a failed instruction. (This
is also why it's a plain proof, not a `#[kani::should_panic]`: a should-panic
encoding would have *started failing* the moment this fix landed.)

## CI

These proofs run **weekly** (and on demand) in the `.github/workflows/kani.yml`
`verify` job, alongside the other `finance/` proof crates — the nonlinear ones
are slow, so the full Kani run is scheduled rather than gating every push/PR. A
fast `cargo test` job runs per push/PR to catch model regressions early.

## Running

```bash
# Plain unit tests (no Kani required):
cargo test

# Formal verification (requires Kani):
cargo install --locked kani-verifier && cargo kani setup   # one-time
cargo kani
```
