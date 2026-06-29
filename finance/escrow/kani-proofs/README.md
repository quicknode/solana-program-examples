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
| `proof_close_offer_conserves_unconditionally` | **Finding** (expected-fail, see below). |
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

`proof_close_offer_conserves_unconditionally` asserts conservation on *every*
path and Kani finds the counterexample `offer == dest == u64::MAX`: the credit
overflows and returns `Err` after the source was already zeroed, so the total
drops from `2·MAX` to `MAX`.

**Severity: not exploitable.** The Solana runtime reverts all account mutations
when an instruction returns `Err`, so the zeroing rolls back; and the
destination is the maker's wallet, which cannot hold anywhere near `u64::MAX`
lamports. The harness documents that conservation holds because of those
*external* guarantees, not the function's own statement ordering — a hardened
version would credit the destination before zeroing the source. The harness
carries `#[kani::should_panic]` so it is green in CI precisely because the
assertion fails as predicted; remove that attribute to see Kani print the
counterexample.

## Running

```bash
# Plain unit tests (no Kani required):
cargo test

# Formal verification (requires Kani):
cargo install --locked kani-verifier && cargo kani setup   # one-time
cargo kani
```
