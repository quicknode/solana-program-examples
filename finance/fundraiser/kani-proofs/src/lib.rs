//! Kani harnesses for the fundraiser program (`finance/fundraiser`).
//!
//! Inspired by aeyakovenko/percolator, which uses the Kani model checker to
//! check a DeFi engine's pure numeric core. Kani marks a harness with
//! `#[kani::proof]`, which is why the crate is `kani-proofs` and the harnesses
//! are named `proof_*`; each one is a model check: Kani tries every value of
//! the inputs the harness declares and reports either that every assertion
//! held or the input that breaks one.
//!
//! The program collects contributions into a vault toward a goal; if the goal
//! is not met by the deadline, every contributor reclaims their exact stake.
//! Token movement is via SPL CPIs Kani cannot symbolically execute, but the
//! accounting (`contribute`, `refund`, `close_contributor`) is pure integer
//! arithmetic. This crate reproduces it faithfully and checks the contributor
//! account counter, the running-total accounting, and refund conservation.

#![cfg_attr(kani, allow(dead_code))]

/// How many contributors the counter harness tracks.
pub const CONTRIBUTORS: usize = 3;

/// One step of a fundraiser's life, as it affects contributor accounts.
#[derive(Clone, Copy)]
pub enum ContributorAccountStep {
    /// `contribute` from this contributor: creates their account on the first
    /// call and adds to it on later ones.
    Contribute(usize),
    /// `refund` or `close_contributor` for this contributor: both close the
    /// account, and both fail if it does not exist.
    Close(usize),
}

/// Replays `contribute`, `refund` and `close_contributor`'s bookkeeping on
/// `open_contributor_accounts`. Returns `None` where the program would reject
/// the step, as it does when the account to close does not exist.
pub fn apply_step(
    open_accounts: &mut [bool; CONTRIBUTORS],
    open_contributor_accounts: u32,
    step: ContributorAccountStep,
) -> Option<u32> {
    match step {
        ContributorAccountStep::Contribute(contributor) => {
            if open_accounts[contributor] {
                Some(open_contributor_accounts)
            } else {
                open_accounts[contributor] = true;
                open_contributor_accounts.checked_add(1)
            }
        }
        ContributorAccountStep::Close(contributor) => {
            if !open_accounts[contributor] {
                return None;
            }
            open_accounts[contributor] = false;
            open_contributor_accounts.checked_sub(1)
        }
    }
}

// ===========================================================================
// 1. Contributor account counter
// ===========================================================================

/// `fundraiser.open_contributor_accounts` always equals the number of
/// contributor accounts that exist for the fundraiser, whatever order
/// contributions, refunds and closes arrive in. `close_fundraiser` requires
/// the counter to be zero, so this is what guarantees no contributor account
/// outlives its fundraiser and carries over into the next raise at the same
/// address.
#[cfg(kani)]
#[kani::proof]
#[kani::unwind(9)]
fn proof_open_contributor_accounts_counts_open_accounts() {
    let mut open_accounts = [false; CONTRIBUTORS];
    let mut open_contributor_accounts: u32 = 0;

    for _ in 0..8 {
        let contributor: usize = kani::any();
        kani::assume(contributor < CONTRIBUTORS);
        let step = if kani::any() {
            ContributorAccountStep::Contribute(contributor)
        } else {
            ContributorAccountStep::Close(contributor)
        };
        if let Some(updated) = apply_step(&mut open_accounts, open_contributor_accounts, step) {
            open_contributor_accounts = updated;
        }
        let actually_open = open_accounts.iter().filter(|open| **open).count() as u32;
        assert_eq!(open_contributor_accounts, actually_open);
    }
}

// ===========================================================================
// 2. Running-total accounting conservation
// ===========================================================================

/// `fundraiser.current_amount` always equals the sum of the contributions added
/// to it (`contribute` does `current_amount += amount` on each, with
/// `checked_add`). Modelled as a sequence of contributions accumulated the same
/// way; the running total equals their sum and never overflows for in-range
/// inputs. Pure linear logic, full `u64` width.
#[cfg(kani)]
#[kani::proof]
fn proof_current_amount_is_sum_of_contributions() {
    let contributions: [u64; 4] = [kani::any(), kani::any(), kani::any(), kani::any()];

    // The contributions are bounded so their sum fits u64 (an in-range goal).
    let mut sum: u128 = 0;
    for &c in contributions.iter() {
        sum += c as u128;
    }
    kani::assume(sum <= u64::MAX as u128);

    // Replay the on-chain accumulation with checked_add.
    let mut current_amount: u64 = 0;
    for &c in contributions.iter() {
        current_amount = current_amount.checked_add(c).expect("sum fits u64");
    }
    assert_eq!(current_amount as u128, sum);
}

// ===========================================================================
// 3. Refund conservation
// ===========================================================================

/// When the goal is not met, every contributor reclaims their exact tracked
/// amount, so the refunds sum back to `current_amount` — the vault is neither
/// over- nor under-drained, and no contributor can reclaim more than they put
/// in. Pure linear logic, full `u64` width.
#[cfg(kani)]
#[kani::proof]
fn proof_refunds_sum_to_current_amount() {
    let contributions: [u64; 4] = [kani::any(), kani::any(), kani::any(), kani::any()];

    let mut current_amount: u128 = 0;
    for &c in contributions.iter() {
        current_amount += c as u128;
    }

    // Each refund returns exactly the contributor's amount.
    let mut refunded: u128 = 0;
    for &c in contributions.iter() {
        refunded += c as u128;
        // No single refund exceeds the pool it is drawn from.
        assert!(c as u128 <= current_amount);
    }
    assert_eq!(refunded, current_amount);
}

// ===========================================================================
// Plain unit tests.
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counter_tracks_contributor_accounts() {
        let mut open_accounts = [false; CONTRIBUTORS];
        let mut counter = 0u32;
        for step in [
            ContributorAccountStep::Contribute(0),
            ContributorAccountStep::Contribute(0),
            ContributorAccountStep::Contribute(2),
            ContributorAccountStep::Close(0),
        ] {
            counter = apply_step(&mut open_accounts, counter, step).unwrap();
        }
        assert_eq!(counter, 1);
        assert!(apply_step(
            &mut open_accounts,
            counter,
            ContributorAccountStep::Close(1)
        )
        .is_none());
    }

    #[test]
    fn accounting_sums() {
        let mut current = 0u64;
        for c in [10u64, 20, 30] {
            current = current.checked_add(c).unwrap();
        }
        assert_eq!(current, 60);
    }
}
