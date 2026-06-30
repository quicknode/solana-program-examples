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
| `proof_close_offer_never_creates_lamports` | **Finding**, proven as the unconditional "no inflation" property (see below). |
| `proof_take_offer_conserves_value` | A take conserves total mint A and total mint B, drains the vault, and pays the maker exactly the price. |
| `proof_take_offer_guard_never_overflows` | The `checked_add` conservation guards in `take_offer` are unreachable dead code. |
| `proof_take_offer_guard_dead_under_spl_invariant` | Same, shown explicitly under the SPL supply invariant. |
| `proof_cancel_offer_returns_all_to_maker` | Cancelling returns every vault token to the maker and conserves mint A. |
| `proof_make_offer_vault_equals_offered` | After a deposit the vault holds exactly the offered amount. |
| `proof_offer_id_le_bytes_roundtrip` / `proof_offer_id_seed_injective` | The PDA `id` seed encoding is a lossless, injective round-trip. |

## Finding: lamport ordering in `close_offer_account`

`utils::close_offer_account` zeroes the offer account's lamports *before* the
fallible `checked_add` that credits the destination:

```rust
**offer_info.lamports.borrow_mut() = 0;                     // (1) zero source
**destination.lamports.borrow_mut() = destination_lamports  // (2) credit dest (may Err)
    .checked_add(offer_lamports)
    .ok_or(EscrowError::ArithmeticOverflow)?;
```

At `offer == dest == u64::MAX` the credit overflows and returns `Err` after the
source was already zeroed, so the total *transiently* drops from `2·MAX` to
`MAX` — lamports are momentarily destroyed on the error path.

**Severity: not exploitable.** The Solana runtime reverts all account mutations
when an instruction returns `Err`, so the zeroing rolls back; and the
destination is the maker's wallet, which cannot hold anywhere near `u64::MAX`
lamports. A hardened version would credit the destination before zeroing the
source.

**How it's encoded.** Rather than a fragile `#[kani::should_panic]` (which would
*start failing* the day someone applies that hardening — fixing the code breaks
the test), `proof_close_offer_never_creates_lamports` proves the
security-relevant direction that holds **unconditionally**: the function can
never *create* lamports (`after <= before` on every path). The transient-
destruction wart is recorded in the harness comment, not as an inverted test.

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
