//! Kani proof harnesses for the vault-strategy program (`finance/vault-strategy`).
//!
//! Inspired by aeyakovenko/percolator, which uses the Kani model checker to
//! prove the mathematical correctness of a DeFi engine's pure numeric core.
//!
//! The program is an ERC4626-style share vault: depositors mint share tokens
//! against the vault's net asset value, and withdrawals burn shares for a
//! proportional slice of every vault balance. A manager fee mints a small slice
//! of shares over time. Token movement is via SPL CPIs Kani cannot symbolically
//! execute, but the share math (`deposit`, `withdraw`, `collect_fees`) is pure
//! integer arithmetic. This crate reproduces it faithfully and proves the
//! invariants the vault's solvency rests on.
//!
//! The program prices shares and pays withdrawals from the holdings it has
//! recorded (`Strategy::usdc_holdings` and `asset_holdings`), never from the
//! vaults' token balances, so tokens donated straight into a vault are outside
//! the fund. The harnesses model a vault as a (recorded, balance) pair and prove
//! what that buys: payouts never exceed the real balance, and a donation cannot
//! dilute the next depositor.
//!
//! Nonlinear 128-bit harnesses use bounded model checking (small symbolic
//! inputs), as percolator does; the share identities are scale-invariant.

#![cfg_attr(kani, allow(dead_code))]

/// `floor((a*b)/d)`, `None` on overflow / zero divisor.
pub fn mul_div_floor(a: u128, b: u128, d: u128) -> Option<u128> {
    if d == 0 {
        return None;
    }
    a.checked_mul(b)?.checked_div(d)
}

/// Proportional withdrawal from one vault's recorded holding:
/// `floor(holding * shares / total)` — the formula `handle_withdraw` applies to
/// the USDC leg and to every basket asset.
pub fn withdraw_amount(balance: u64, shares_burned: u64, total_shares: u64) -> Option<u64> {
    mul_div_floor(balance as u128, shares_burned as u128, total_shares as u128)?
        .try_into()
        .ok()
}

/// Shares minted for a deposit: `floor(usdc_amount * total_shares / nav)`, where
/// `nav` is valued from recorded holdings (`handle_deposit`; the first deposit,
/// `total_shares == 0`, mints 1:1).
pub fn deposit_shares(usdc_amount: u64, total_shares: u64, nav: u64) -> Option<u64> {
    if total_shares == 0 {
        return Some(usdc_amount);
    }
    mul_div_floor(usdc_amount as u128, total_shares as u128, nav as u128)?
        .try_into()
        .ok()
}

// ===========================================================================
// 1. Withdrawal solvency
// ===========================================================================

/// A withdrawal can never take more of any vault balance than it holds. Because
/// the burned shares are at most the total supply, the proportional slice
/// `floor(balance * shares / total)` is `<= balance`. This holds for the USDC
/// leg and for every in-kind asset leg, so a withdrawal can never overdraw a
/// vault — the core solvency property.
#[cfg(kani)]
#[kani::proof]
#[kani::solver(cadical)]
fn proof_withdraw_within_balance() {
    let balance: u64 = kani::any();
    let shares_burned: u64 = kani::any();
    let total_shares: u64 = kani::any();

    // Bounded model checking (nonlinear `balance * shares`, symbolic divisor
    // `total_shares`).
    kani::assume(balance as u128 <= 255);
    kani::assume(total_shares >= 1 && total_shares <= 255);
    kani::assume(shares_burned <= total_shares); // can't burn more than supply

    let out = withdraw_amount(balance, shares_burned, total_shares).expect("computes");
    assert!(out <= balance);
    // And withdrawing the entire supply takes exactly the whole balance.
    if shares_burned == total_shares {
        assert_eq!(out, balance);
    }
}

// ===========================================================================
// 2. Deposit -> withdraw round-trip cannot extract value
// ===========================================================================

/// In a USDC-only vault (NAV == vault USDC, no basket assets), depositing and
/// immediately withdrawing the minted shares never returns more USDC than was
/// deposited. Both legs floor in the protocol's favour, so a deposit/withdraw
/// round-trip is never profitable — there is no rounding attack that mints
/// shares worth more than they cost.
#[cfg(kani)]
#[kani::proof]
#[kani::solver(cadical)]
fn proof_deposit_withdraw_cannot_extract() {
    let amount: u64 = kani::any();
    let total_shares: u64 = kani::any();
    let nav: u64 = kani::any(); // == vault USDC balance for a USDC-only vault

    // Bounded model checking; an established vault has positive supply and NAV.
    kani::assume(amount <= 31);
    kani::assume(total_shares >= 1 && total_shares <= 31);
    kani::assume(nav >= 1 && nav <= 31);

    let minted = deposit_shares(amount, total_shares, nav).expect("computes");

    // State after the deposit.
    let new_total = (total_shares as u128) + (minted as u128);
    let new_vault = (nav as u128) + (amount as u128);

    // Withdraw exactly the freshly minted shares.
    let back = mul_div_floor(new_vault, minted as u128, new_total).expect("computes");
    assert!(back <= amount as u128); // round-trip never profitable
}

// ===========================================================================
// 3. Recorded holdings never exceed the vault's balance
// ===========================================================================

/// One vault as the program sees it (`recorded`) and as the token program does
/// (`balance`). A deposit adds to both, a donation adds to the balance only, and
/// a withdrawal pays `withdraw_amount` of the recorded holding out of both. If
/// `recorded <= balance` holds before each step it holds after it, and every
/// payout is covered by the real balance: the program can never promise tokens
/// the vault does not hold, however much is donated.
#[cfg(kani)]
#[kani::proof]
#[kani::solver(cadical)]
fn proof_recorded_holdings_never_exceed_balance() {
    let recorded: u64 = kani::any();
    let balance: u64 = kani::any();
    let deposit: u64 = kani::any();
    let donation: u64 = kani::any();
    let shares_burned: u64 = kani::any();
    let total_shares: u64 = kani::any();

    kani::assume(recorded <= balance && balance <= 255);
    kani::assume(deposit <= 255 && donation <= 255);
    kani::assume(total_shares >= 1 && total_shares <= 255);
    kani::assume(shares_burned <= total_shares);

    // Deposit: the swap output is recorded and lands in the vault.
    let recorded = recorded + deposit;
    let balance = balance + deposit;
    assert!(recorded <= balance);

    // Donation: tokens land in the vault and nothing is recorded.
    let balance = balance + donation;
    assert!(recorded <= balance);

    // Withdrawal: paid from the recorded holding, out of the real balance.
    let payout = withdraw_amount(recorded, shares_burned, total_shares).expect("computes");
    assert!(payout <= recorded);
    assert!(payout <= balance);
    assert!(recorded - payout <= balance - payout);
}

// ===========================================================================
// 4. A donation cannot dilute the next deposit
// ===========================================================================

/// The inflation attack against recorded holdings, in a USDC-only vault: an
/// attacker's first deposit mints one share per minor unit, a donation of any
/// size lands in the vault, and then the victim deposits. The donation is not in
/// the recorded NAV, so the victim's shares are exactly their deposit, the same
/// as with no donation, and withdrawing them returns every minor unit.
#[cfg(kani)]
#[kani::proof]
#[kani::solver(cadical)]
fn proof_donation_cannot_dilute_next_deposit() {
    let attacker_deposit: u64 = kani::any();
    let donation: u64 = kani::any();
    let victim_deposit: u64 = kani::any();

    kani::assume(attacker_deposit >= 1 && attacker_deposit <= 31);
    kani::assume(victim_deposit <= 31);

    let attacker_shares = deposit_shares(attacker_deposit, 0, 0).expect("computes");
    let recorded = attacker_deposit;
    let _balance = recorded as u128 + donation as u128; // counted nowhere below

    let victim_shares =
        deposit_shares(victim_deposit, attacker_shares, recorded).expect("computes");
    assert_eq!(victim_shares, victim_deposit);

    let total = attacker_shares + victim_shares;
    let back = withdraw_amount(recorded + victim_deposit, victim_shares, total).expect("computes");
    assert_eq!(back, victim_deposit);
}

// ===========================================================================
// 5. Manager fee dilution is bounded
// ===========================================================================

/// The time-based manager fee mints
/// `fee_shares = floor(total_shares * fee_bps * elapsed / (10_000 * SECONDS_PER_YEAR))`.
/// Over at most one year (`elapsed <= SECONDS_PER_YEAR`) with a valid fee rate
/// (`fee_bps <= 10_000`), the combined numerator factor `fee_bps * elapsed` is
/// `<= 10_000 * SECONDS_PER_YEAR`, so `fee_shares <= total_shares`: the manager
/// can never mint more than a 100%-per-year dilution. Modelled with the combined
/// `numerator_factor <= denominator` (the constraint the two bounds imply).
#[cfg(kani)]
#[kani::proof]
#[kani::solver(cadical)]
fn proof_fee_shares_bounded_by_supply() {
    let total_shares: u64 = kani::any();
    let numerator_factor: u128 = kani::any(); // fee_bps * elapsed
    let denominator: u128 = kani::any(); // 10_000 * SECONDS_PER_YEAR

    kani::assume(total_shares as u128 <= 255);
    kani::assume(denominator >= 1 && denominator <= 255);
    // fee_bps <= 10_000 and elapsed <= SECONDS_PER_YEAR together give:
    kani::assume(numerator_factor <= denominator);

    let fee_shares =
        mul_div_floor(total_shares as u128, numerator_factor, denominator).expect("computes");
    assert!(fee_shares <= total_shares as u128); // <= 100%/year dilution
}

// ===========================================================================
// Plain unit tests.
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn withdraw_proportional() {
        // Burn half the supply -> get half the balance.
        assert_eq!(withdraw_amount(1000, 50, 100).unwrap(), 500);
        // Burn all -> get all.
        assert_eq!(withdraw_amount(1000, 100, 100).unwrap(), 1000);
    }

    #[test]
    fn deposit_first_is_one_to_one() {
        assert_eq!(deposit_shares(500, 0, 0).unwrap(), 500);
    }

    #[test]
    fn donation_does_not_dilute_next_deposit() {
        // The attack from the book: one minor unit deposited, 1,000 USDC donated
        // (and not recorded), then a 1,000 USDC deposit.
        let attacker = deposit_shares(1, 0, 0).unwrap();
        let victim = deposit_shares(1_000_000_000, attacker, 1).unwrap();
        assert_eq!(victim, 1_000_000_000);
        let back = withdraw_amount(1 + 1_000_000_000, victim, attacker + victim).unwrap();
        assert_eq!(back, 1_000_000_000);
    }

    #[test]
    fn round_trip_not_profitable() {
        let minted = deposit_shares(100, 200, 150).unwrap();
        let back =
            mul_div_floor((150 + 100) as u128, minted as u128, (200 + minted) as u128).unwrap();
        assert!(back <= 100);
    }
}
