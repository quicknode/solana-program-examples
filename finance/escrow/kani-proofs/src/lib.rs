//! Kani proof harnesses for the Solana escrow program.
//!
//! Inspired by aeyakovenko/percolator, which uses Kani to prove the
//! mathematical correctness of a risk engine's pure computational core.
//!
//! Kani is a bit-precise model checker: a `#[kani::proof]` harness explores
//! *every* possible value of its `kani::any()` inputs and reports any input
//! for which an `assert!` can fail (or for which arithmetic overflows, etc.).
//!
//! ## Why model instead of verifying the program crate directly
//!
//! The escrow program does almost no arithmetic itself: it hands the actual
//! token movement to the SPL token program through cross-program invocations
//! (`invoke` / `invoke_signed`). Those CPIs are opaque syscalls that Kani
//! cannot symbolically execute, and the program types (`AccountInfo`, `Pubkey`,
//! borsh buffers) are awkward to make symbolic. So — exactly like percolator,
//! which verifies a self-contained library — we model the escrow's verifiable
//! core as pure functions and prove the invariants the on-chain code relies on:
//!
//!   1. `token_transfer`     - faithful model of an SPL `transfer_checked`.
//!   2. lamport closing       - models `utils::close_offer_account`.
//!   3. swap conservation     - models `take_offer` / `cancel_offer` balance math.
//!   4. seed round-trip       - the `id.to_le_bytes()` PDA seed math.
//!
//! Each model mirrors the real code's arithmetic and statement ordering so the
//! proofs say something meaningful about the deployed program.

#![cfg_attr(kani, allow(dead_code))]

// ---------------------------------------------------------------------------
// Token transfer model
// ---------------------------------------------------------------------------

/// Why a modeled token transfer failed.
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum TokenError {
    /// `from` does not hold `amount` tokens (SPL: `InsufficientFunds`).
    InsufficientFunds,
    /// Crediting `to` would exceed `u64::MAX` (SPL: arithmetic `Overflow`).
    Overflow,
}

/// Faithful model of a single SPL `spl_token::transfer_checked`.
///
/// The real SPL token program performs *checked* arithmetic: it debits `from`
/// only if it holds enough, credits `to` only if the sum fits in `u64`, and the
/// two operations together conserve the total. This models exactly that.
pub fn token_transfer(from: &mut u64, to: &mut u64, amount: u64) -> Result<(), TokenError> {
    let new_from = from.checked_sub(amount).ok_or(TokenError::InsufficientFunds)?;
    let new_to = to.checked_add(amount).ok_or(TokenError::Overflow)?;
    *from = new_from;
    *to = new_to;
    Ok(())
}

/// A token transfer either fails leaving balances untouched, or succeeds
/// conserving the total balance of the two accounts. This is the foundational
/// invariant every escrow conservation check leans on.
#[cfg(kani)]
#[kani::proof]
fn proof_token_transfer_conserves() {
    let from0: u64 = kani::any();
    let to0: u64 = kani::any();
    let amount: u64 = kani::any();

    let mut from = from0;
    let mut to = to0;
    let total_before = from0 as u128 + to0 as u128;

    match token_transfer(&mut from, &mut to, amount) {
        Ok(()) => {
            // No tokens created or destroyed.
            assert_eq!(from as u128 + to as u128, total_before);
            // The mover lost exactly what the receiver gained.
            assert_eq!(from, from0 - amount);
            assert_eq!(to, to0 + amount);
        }
        Err(_) => {
            // Failure must be atomic: balances are untouched.
            assert_eq!(from, from0);
            assert_eq!(to, to0);
        }
    }
}

// ---------------------------------------------------------------------------
// Lamport accounting: utils::close_offer_account
// ---------------------------------------------------------------------------
//
// The native program closes the offer account like this (native/.../utils.rs):
//
//     let offer_lamports = offer_info.lamports();
//     let destination_lamports = destination.lamports();
//     **offer_info.lamports.borrow_mut() = 0;                    // (1) zero source
//     **destination.lamports.borrow_mut() = destination_lamports // (2) credit dest
//         .checked_add(offer_lamports)
//         .ok_or(EscrowError::ArithmeticOverflow)?;
//
// This model preserves that exact statement ordering, including the fact that
// the source is zeroed *before* the (fallible) credit of the destination.

/// Lamport-overflow error, mirroring `EscrowError::ArithmeticOverflow`.
#[derive(Debug, PartialEq, Eq)]
pub struct LamportOverflow;

pub fn close_offer_account(offer: &mut u64, destination: &mut u64) -> Result<(), LamportOverflow> {
    let offer_lamports = *offer;
    let destination_lamports = *destination;
    *offer = 0; // (1)
    *destination = destination_lamports
        .checked_add(offer_lamports)
        .ok_or(LamportOverflow)?; // (2)
    Ok(())
}

/// Happy path: when the destination can absorb the offer's rent, closing the
/// account conserves total lamports and leaves the offer empty.
#[cfg(kani)]
#[kani::proof]
fn proof_close_offer_conserves_on_success() {
    let offer0: u64 = kani::any();
    let dest0: u64 = kani::any();
    kani::assume(dest0.checked_add(offer0).is_some()); // success precondition

    let mut offer = offer0;
    let mut dest = dest0;
    let r = close_offer_account(&mut offer, &mut dest);

    assert!(r.is_ok());
    assert_eq!(offer, 0);
    assert_eq!(dest, dest0 + offer0);
    assert_eq!(offer as u128 + dest as u128, offer0 as u128 + dest0 as u128);
}

/// SAFETY (no inflation): `close_offer_account` never *creates* lamports on any
/// path. On success it conserves the total exactly (see
/// `proof_close_offer_conserves_on_success`); on the overflow error path it has
/// already zeroed the source but not yet credited the destination, so the total
/// is strictly lower. Either way `after <= before`, so the function can never
/// inflate the lamport supply — the security-relevant direction, and it holds
/// unconditionally.
///
/// DOCUMENTED WART (not a live bug): the error path *transiently destroys*
/// lamports, because the source is zeroed before the fallible `checked_add` that
/// credits the destination (statements (1) then (2) above). Kani's witness is
/// `offer0 == dest0 == u64::MAX`. On-chain this is invisible: the Solana runtime
/// reverts all account mutations when an instruction returns `Err`, and the
/// destination (the maker's wallet) cannot hold anywhere near `u64::MAX`
/// lamports, so the overflow branch is unreachable in practice. A one-line
/// hardening — credit the destination *before* zeroing the source — would make
/// conservation hold with equality on every path; we assert the weaker-but-
/// unconditional "no inflation" here rather than encode the wart as an
/// (inverted, fragile) `#[kani::should_panic]`.
#[cfg(kani)]
#[kani::proof]
fn proof_close_offer_never_creates_lamports() {
    let offer0: u64 = kani::any();
    let dest0: u64 = kani::any();

    let mut offer = offer0;
    let mut dest = dest0;
    let total_before = offer0 as u128 + dest0 as u128;

    let _ = close_offer_account(&mut offer, &mut dest);

    // No path ever increases the total lamports: the function cannot inflate.
    assert!(offer as u128 + dest as u128 <= total_before);
}

// ---------------------------------------------------------------------------
// take_offer: the atomic swap
// ---------------------------------------------------------------------------
//
// take_offer performs two transfers:
//   (B) taker -> maker  of `token_b_wanted_amount` of mint B
//   (A) vault -> taker   of the vault's entire mint A balance
// then re-reads balances and asserts (with checked_add) that each receiver
// gained exactly the moved amount, else returns TokenConservationViolation.

/// The four token balances that `take_offer` moves.
#[derive(Debug, Clone, Copy)]
pub struct TakeBalances {
    pub taker_a: u64,
    pub taker_b: u64,
    pub maker_b: u64,
    pub vault_a: u64,
}

#[derive(Debug, PartialEq, Eq)]
pub enum TakeError {
    Token(TokenError),
    /// The post-transfer conservation check itself overflowed
    /// (`EscrowError::ArithmeticOverflow`).
    ConservationOverflow,
}

/// Faithful model of the token movement + conservation checks in
/// `take_offer::process`.
pub fn take_offer(b: &mut TakeBalances, wanted_b: u64) -> Result<(), TakeError> {
    let taker_a_before = b.taker_a;
    let maker_b_before = b.maker_b;
    let vault_a = b.vault_a;

    // (B) taker pays the maker the wanted mint-B amount.
    token_transfer(&mut b.taker_b, &mut b.maker_b, wanted_b).map_err(TakeError::Token)?;
    // (A) vault releases all of its mint A to the taker.
    token_transfer(&mut b.vault_a, &mut b.taker_a, vault_a).map_err(TakeError::Token)?;

    // The program's own post-conditions (these use checked_add on-chain).
    let expected_taker_a = taker_a_before
        .checked_add(vault_a)
        .ok_or(TakeError::ConservationOverflow)?;
    let expected_maker_b = maker_b_before
        .checked_add(wanted_b)
        .ok_or(TakeError::ConservationOverflow)?;

    // On-chain these are `if != { return Err(TokenConservationViolation) }`.
    assert_eq!(b.taker_a, expected_taker_a);
    assert_eq!(b.maker_b, expected_maker_b);
    Ok(())
}

/// Core swap correctness: on success, total mint A and total mint B are each
/// conserved, the vault is drained, and value flows in the intended direction.
#[cfg(kani)]
#[kani::proof]
fn proof_take_offer_conserves_value() {
    let taker_a: u64 = kani::any();
    let taker_b: u64 = kani::any();
    let maker_b: u64 = kani::any();
    let vault_a: u64 = kani::any();
    let wanted_b: u64 = kani::any();

    let mut b = TakeBalances { taker_a, taker_b, maker_b, vault_a };
    let total_a_before = taker_a as u128 + vault_a as u128;
    let total_b_before = taker_b as u128 + maker_b as u128;

    if take_offer(&mut b, wanted_b).is_ok() {
        // Conservation across both mints: nothing minted, nothing burned.
        assert_eq!(b.taker_a as u128 + b.vault_a as u128, total_a_before);
        assert_eq!(b.taker_b as u128 + b.maker_b as u128, total_b_before);
        // The vault is fully drained to the taker.
        assert_eq!(b.vault_a, 0);
        assert_eq!(b.taker_a, taker_a + vault_a);
        // The maker received exactly the price.
        assert_eq!(b.maker_b, maker_b + wanted_b);
        assert_eq!(b.taker_b, taker_b - wanted_b);
    }
}

/// FINDING (this harness PASSES, and that is the finding): the on-chain
/// `checked_add` conservation guards in `take_offer` are unreachable defensive
/// code. Their `ArithmeticOverflow` arm can never be taken, because the
/// preceding `transfer_checked` (modeled by `token_transfer`, which itself does
/// a `checked_add`) has *already* established that `taker_a_before + vault_a`
/// and `maker_b_before + wanted_b` fit in `u64` — otherwise the transfer would
/// have failed first. So `.ok_or(ArithmeticOverflow)` and the subsequent
/// `TokenConservationViolation` comparison are belt-and-suspenders checks that
/// cannot fire. Kani proves this directly, without needing to assume any
/// external SPL invariant (the model already encodes it).
#[cfg(kani)]
#[kani::proof]
fn proof_take_offer_guard_never_overflows() {
    let taker_a: u64 = kani::any();
    let taker_b: u64 = kani::any();
    let maker_b: u64 = kani::any();
    let vault_a: u64 = kani::any();
    let wanted_b: u64 = kani::any();

    let mut b = TakeBalances { taker_a, taker_b, maker_b, vault_a };
    assert_ne!(take_offer(&mut b, wanted_b), Err(TakeError::ConservationOverflow));
}

/// Companion to the finding above: once we assume the SPL invariant that a
/// receiver's post-balance fits in `u64` (which is exactly the precondition
/// under which the `transfer_checked` calls succeed), the `ConservationOverflow`
/// arm is provably unreachable. This proof PASSES, confirming the guard is dead
/// code rather than a real bug.
#[cfg(kani)]
#[kani::proof]
fn proof_take_offer_guard_dead_under_spl_invariant() {
    let taker_a: u64 = kani::any();
    let taker_b: u64 = kani::any();
    let maker_b: u64 = kani::any();
    let vault_a: u64 = kani::any();
    let wanted_b: u64 = kani::any();

    // SPL token invariant: a successful transfer means the receiver's resulting
    // balance fit in u64. That is precisely `before + amount <= u64::MAX`.
    kani::assume((taker_a as u128 + vault_a as u128) <= u64::MAX as u128);
    kani::assume((maker_b as u128 + wanted_b as u128) <= u64::MAX as u128);

    let mut b = TakeBalances { taker_a, taker_b, maker_b, vault_a };
    assert_ne!(take_offer(&mut b, wanted_b), Err(TakeError::ConservationOverflow));
}

// ---------------------------------------------------------------------------
// cancel_offer: maker reclaims the vault
// ---------------------------------------------------------------------------

#[derive(Debug, PartialEq, Eq)]
pub enum CancelError {
    Token(TokenError),
    ConservationOverflow,
}

/// Models `cancel_offer::process`: the vault returns its entire mint-A balance
/// to the maker, then the program checks the maker gained exactly that amount.
pub fn cancel_offer(maker_a: &mut u64, vault_a: &mut u64) -> Result<(), CancelError> {
    let maker_a_before = *maker_a;
    let amount = *vault_a;

    token_transfer(vault_a, maker_a, amount).map_err(CancelError::Token)?;

    let expected = maker_a_before
        .checked_add(amount)
        .ok_or(CancelError::ConservationOverflow)?;
    assert_eq!(*maker_a, expected);
    Ok(())
}

/// Cancelling returns every vault token to the maker and conserves mint A.
#[cfg(kani)]
#[kani::proof]
fn proof_cancel_offer_returns_all_to_maker() {
    let maker_a: u64 = kani::any();
    let vault_a: u64 = kani::any();

    let mut m = maker_a;
    let mut v = vault_a;
    let total_before = maker_a as u128 + vault_a as u128;

    if cancel_offer(&mut m, &mut v).is_ok() {
        assert_eq!(v, 0); // vault drained
        assert_eq!(m, maker_a + vault_a); // maker made whole
        assert_eq!(m as u128 + v as u128, total_before); // conservation
    }
}

// ---------------------------------------------------------------------------
// make_offer: deposit equals the vault balance
// ---------------------------------------------------------------------------

#[derive(Debug, PartialEq, Eq)]
pub enum MakeError {
    Token(TokenError),
    /// `EscrowError::TokenConservationViolation`.
    ConservationViolation,
}

/// Models `make_offer`'s deposit + conservation check: the maker funds the
/// (empty) vault with `offered`, then the program requires the vault to hold
/// exactly `offered`.
pub fn make_offer(maker_a: &mut u64, vault_a: &mut u64, offered: u64) -> Result<(), MakeError> {
    // The vault is a freshly created ATA, so it starts empty.
    assert_eq!(*vault_a, 0);
    token_transfer(maker_a, vault_a, offered).map_err(MakeError::Token)?;
    if *vault_a != offered {
        return Err(MakeError::ConservationViolation);
    }
    Ok(())
}

/// After a successful deposit the vault holds exactly the offered amount and the
/// conservation check never trips.
#[cfg(kani)]
#[kani::proof]
fn proof_make_offer_vault_equals_offered() {
    let maker_a: u64 = kani::any();
    let offered: u64 = kani::any();

    let mut m = maker_a;
    let mut v: u64 = 0; // fresh vault
    let total_before = maker_a as u128 + 0u128;

    match make_offer(&mut m, &mut v, offered) {
        Ok(()) => {
            assert_eq!(v, offered);
            assert_eq!(m as u128 + v as u128, total_before); // mint A conserved
        }
        Err(MakeError::Token(TokenError::InsufficientFunds)) => {
            // Only reason to fail: maker did not have `offered` tokens.
            assert!(offered > maker_a);
        }
        Err(e) => panic!("unexpected make_offer error: {:?}", e),
    }
}

// ---------------------------------------------------------------------------
// PDA seed math: id round-trip
// ---------------------------------------------------------------------------
//
// Both make_offer (find_program_address) and take/cancel (create_program_address)
// derive the offer PDA from `id.to_le_bytes()`. The stored `offer.id` is later
// re-encoded the same way; correctness requires the byte encoding to be a
// faithful, injective round-trip so the *same* id always maps to the *same*
// vault/offer addresses.

/// `to_le_bytes` / `from_le_bytes` is a lossless round-trip for every id.
#[cfg(kani)]
#[kani::proof]
fn proof_offer_id_le_bytes_roundtrip() {
    let id: u64 = kani::any();
    assert_eq!(u64::from_le_bytes(id.to_le_bytes()), id);
}

/// The encoding is injective: distinct ids never collide on the seed bytes,
/// so two different offers can never derive the same PDA from their id.
#[cfg(kani)]
#[kani::proof]
fn proof_offer_id_seed_injective() {
    let a: u64 = kani::any();
    let b: u64 = kani::any();
    kani::assume(a != b);
    assert_ne!(a.to_le_bytes(), b.to_le_bytes());
}

// ---------------------------------------------------------------------------
// Plain unit tests so the crate is also useful without Kani installed.
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transfer_conserves() {
        let mut from = 100u64;
        let mut to = 5u64;
        token_transfer(&mut from, &mut to, 30).unwrap();
        assert_eq!((from, to), (70, 35));
    }

    #[test]
    fn take_offer_swaps() {
        let mut b = TakeBalances { taker_a: 0, taker_b: 50, maker_b: 0, vault_a: 10 };
        take_offer(&mut b, 7).unwrap();
        assert_eq!(b.vault_a, 0);
        assert_eq!(b.taker_a, 10);
        assert_eq!(b.maker_b, 7);
        assert_eq!(b.taker_b, 43);
    }

    #[test]
    fn close_conserves() {
        let mut offer = 900u64;
        let mut dest = 100u64;
        close_offer_account(&mut offer, &mut dest).unwrap();
        assert_eq!((offer, dest), (0, 1000));
    }
}
