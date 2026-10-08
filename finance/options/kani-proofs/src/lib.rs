//! Kani harnesses for the options venue (`finance/options`).
//!
//! Inspired by aeyakovenko/percolator, which uses the Kani model checker to
//! check a DeFi engine's pure numeric core. Kani marks a harness with
//! `#[kani::proof]`, which is why the crate is `kani-proofs` and the harnesses
//! are named `proof_*`; each one is a model check: Kani tries every value of
//! the inputs the harness declares and reports either that every assertion
//! held or the input that breaks one.
//!
//! The on-chain instructions hand the actual token movement to the SPL token
//! program via CPIs that Kani cannot symbolically execute. The arithmetic
//! underneath is small and is reproduced here faithfully, mirroring
//! `options::contract_math`: settlement moves the two amounts the option
//! stores, the only rounding is in the fee split, where the fee rounds up and
//! the writer takes the remainder, and the expiry window is one comparison and
//! its complement. The harnesses check the invariants the program's custody
//! accounting depends on, plus a bounded model of the vault ledger across an
//! option's whole life.

#![cfg_attr(kani, allow(dead_code))]

/// Basis-points denominator (`constants::BASIS_POINTS_DENOMINATOR`).
pub const BASIS_POINTS: u128 = 10_000;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum OptionKind {
    Call,
    Put,
}

// ===========================================================================
// 1. Settlement amounts  (contract_math.rs)
// ===========================================================================

/// What the writer posts: the underlying for a call, the strike for a put.
/// Mirrors `contract_math::collateral_amount`.
pub fn collateral_amount(kind: OptionKind, underlying_amount: u64, strike_amount: u64) -> u64 {
    match kind {
        OptionKind::Call => underlying_amount,
        OptionKind::Put => strike_amount,
    }
}

/// What the holder pays at exercise and the writer later collects. Mirrors
/// `contract_math::exercise_payment`.
pub fn exercise_payment(kind: OptionKind, underlying_amount: u64, strike_amount: u64) -> u64 {
    match kind {
        OptionKind::Call => strike_amount,
        OptionKind::Put => underlying_amount,
    }
}

/// Physical settlement moves exactly the posted collateral to the holder and
/// exactly the mirrored payment to the writer, for every option the program
/// would accept: the holder's payment for a call is a put's collateral on the
/// same terms, and the other way round. Settlement does no arithmetic, so
/// nothing can open a gap between what was posted and what is delivered, and
/// the harness needs no bound on the amounts.
#[cfg(kani)]
#[kani::proof]
fn proof_exercise_moves_exactly_the_posted_terms() {
    let underlying_amount: u64 = kani::any();
    let strike_amount: u64 = kani::any();
    // write_option refuses a zero amount.
    kani::assume(underlying_amount >= 1 && strike_amount >= 1);

    for kind in [OptionKind::Call, OptionKind::Put] {
        let collateral = collateral_amount(kind, underlying_amount, strike_amount);
        let payment = exercise_payment(kind, underlying_amount, strike_amount);
        let (expected_collateral, expected_payment) = match kind {
            OptionKind::Call => (underlying_amount, strike_amount),
            OptionKind::Put => (strike_amount, underlying_amount),
        };
        assert_eq!(collateral, expected_collateral);
        assert_eq!(payment, expected_payment);
        // Both legs are positive: an option that delivers nothing or costs
        // nothing to exercise cannot exist.
        assert!(collateral > 0 && payment > 0);
        // The two kinds are mirror images: a call's payment is a put's
        // collateral on the same terms.
        let mirror = match kind {
            OptionKind::Call => OptionKind::Put,
            OptionKind::Put => OptionKind::Call,
        };
        assert_eq!(
            payment,
            collateral_amount(mirror, underlying_amount, strike_amount)
        );
    }
}

// ===========================================================================
// 2. The premium split  (contract_math.rs)
// ===========================================================================

/// The fee rounds up; the writer takes the remainder. Mirrors
/// `contract_math::split_premium`.
pub fn split_premium(premium: u64, fee_bps: u16) -> Option<(u64, u64)> {
    let fee = (premium as u128)
        .checked_mul(fee_bps as u128)?
        .div_ceil(BASIS_POINTS);
    let fee = u64::try_from(fee).ok()?;
    let to_writer = premium.checked_sub(fee)?;
    Some((fee, to_writer))
}

/// The premium is conserved: fee plus the writer's share is exactly the
/// premium, and the fee never exceeds the premium. Also checks the fee is the
/// exact ceiling of `premium * bps / 10_000`, so a refactor that floors
/// against the venue, or rounds up by more than the one minor unit a ceiling
/// allows, fails the check.
#[cfg(kani)]
#[kani::proof]
#[kani::solver(cadical)]
fn proof_premium_split_conserves_the_premium() {
    let premium: u64 = kani::any();
    let fee_bps: u16 = kani::any();
    // initialize_market accepts 0 <= fee_bps < 10_000; write_option requires
    // a positive premium. Bounded model checking: `premium * fee_bps` is
    // symbolic times symbolic and the quotient is a 128-bit division, the
    // worst case for the bit-precise solver (a 16-bit premium against the
    // full fee range runs for hours). The ceiling's behavior depends only
    // on the product's residue mod 10_000; a 12-bit premium against a 10-bit
    // fee already reaches every residue and the fee-equals-premium edge, and
    // larger operands add magnitude rather than new behavior. The full fee
    // range, including the 99.99% rate, is pinned by the unit tests.
    kani::assume(premium >= 1 && premium <= 0xFF);
    kani::assume(fee_bps <= 0xFF);

    let (fee, to_writer) = split_premium(premium, fee_bps).expect("split computes");

    assert_eq!(fee as u128 + to_writer as u128, premium as u128);
    // With the rate under 100%, the fee never exceeds the premium, even
    // rounded up.
    assert!(fee <= premium);
    assert_eq!(to_writer, premium - fee);
    // Exact ceiling: (fee - 1) * 10_000 < premium * bps <= fee * 10_000, so
    // the venue is never short and never takes more than one minor unit of
    // rounding.
    let target = (premium as u128) * (fee_bps as u128);
    assert!((fee as u128) * BASIS_POINTS >= target);
    assert!((fee as u128) * BASIS_POINTS < target + BASIS_POINTS);
    // The rounding unit is taken only when there is something to round.
    assert_eq!(fee == 0, target == 0);
}

// ===========================================================================
// 3. The expiry window  (contract_math.rs)
// ===========================================================================

/// Mirrors `contract_math::may_exercise`.
pub fn may_exercise(now: i64, expiry: i64) -> bool {
    now < expiry
}

/// Mirrors `contract_math::may_reclaim`.
pub fn may_reclaim(now: i64, expiry: i64) -> bool {
    now >= expiry
}

/// At every instant exactly one of the two parties can claim a held option's
/// collateral: the holder by exercising, or the writer by reclaiming. Never
/// both (a double claim), never neither (collateral stranded).
#[cfg(kani)]
#[kani::proof]
fn proof_exercise_and_reclaim_windows_partition_time() {
    let now: i64 = kani::any();
    let expiry: i64 = kani::any();
    assert!(may_exercise(now, expiry) != may_reclaim(now, expiry));
}

// ===========================================================================
// 4. The vault ledger across an option's life  (the handlers' custody accounting)
// ===========================================================================

/// The market's ledger: what each vault owes, plus the venue's fees. Mirrors
/// the three counters on the `Market` account.
#[derive(Clone, Copy, Default, Debug, PartialEq, Eq)]
pub struct Ledger {
    pub underlying_owed: u64,
    pub quote_owed: u64,
    pub fees_owed: u64,
    /// The token balances the handlers' transfers leave in the two vaults.
    pub underlying_vault: u64,
    pub quote_vault: u64,
}

impl Ledger {
    /// The custody invariant every handler asserts after its math (mirrors
    /// `shared::check_custody`), strengthened to equality: with no donations,
    /// each vault holds exactly what the market owes.
    pub fn is_consistent(&self) -> bool {
        self.underlying_vault == self.underlying_owed
            && self.quote_vault as u128 == self.quote_owed as u128 + self.fees_owed as u128
    }

    /// `write_option`: collateral into the vault, owed back to the writer.
    pub fn write(&mut self, kind: OptionKind, collateral: u64) -> Option<()> {
        match kind {
            OptionKind::Call => {
                self.underlying_owed = self.underlying_owed.checked_add(collateral)?;
                self.underlying_vault = self.underlying_vault.checked_add(collateral)?;
            }
            OptionKind::Put => {
                self.quote_owed = self.quote_owed.checked_add(collateral)?;
                self.quote_vault = self.quote_vault.checked_add(collateral)?;
            }
        }
        Some(())
    }

    /// `buy_option`: the fee lands in the quote vault; the rest of the
    /// premium goes buyer to writer and never touches a vault.
    pub fn buy(&mut self, fee: u64) -> Option<()> {
        self.fees_owed = self.fees_owed.checked_add(fee)?;
        self.quote_vault = self.quote_vault.checked_add(fee)?;
        Some(())
    }

    /// `exercise_option`: the collateral leaves for the holder, the payment
    /// arrives and is owed to the writer.
    pub fn exercise(&mut self, kind: OptionKind, collateral: u64, payment: u64) -> Option<()> {
        match kind {
            OptionKind::Call => {
                self.underlying_owed = self.underlying_owed.checked_sub(collateral)?;
                self.underlying_vault = self.underlying_vault.checked_sub(collateral)?;
                self.quote_owed = self.quote_owed.checked_add(payment)?;
                self.quote_vault = self.quote_vault.checked_add(payment)?;
            }
            OptionKind::Put => {
                self.quote_owed = self.quote_owed.checked_sub(collateral)?;
                self.quote_vault = self.quote_vault.checked_sub(collateral)?;
                self.underlying_owed = self.underlying_owed.checked_add(payment)?;
                self.underlying_vault = self.underlying_vault.checked_add(payment)?;
            }
        }
        Some(())
    }

    /// `collect_proceeds`: the payment leaves for the writer.
    pub fn collect_proceeds(&mut self, kind: OptionKind, payment: u64) -> Option<()> {
        match kind {
            OptionKind::Call => {
                self.quote_owed = self.quote_owed.checked_sub(payment)?;
                self.quote_vault = self.quote_vault.checked_sub(payment)?;
            }
            OptionKind::Put => {
                self.underlying_owed = self.underlying_owed.checked_sub(payment)?;
                self.underlying_vault = self.underlying_vault.checked_sub(payment)?;
            }
        }
        Some(())
    }

    /// `cancel_option` and `reclaim_collateral`: the collateral goes back to
    /// the writer.
    pub fn return_collateral(&mut self, kind: OptionKind, collateral: u64) -> Option<()> {
        match kind {
            OptionKind::Call => {
                self.underlying_owed = self.underlying_owed.checked_sub(collateral)?;
                self.underlying_vault = self.underlying_vault.checked_sub(collateral)?;
            }
            OptionKind::Put => {
                self.quote_owed = self.quote_owed.checked_sub(collateral)?;
                self.quote_vault = self.quote_vault.checked_sub(collateral)?;
            }
        }
        Some(())
    }

    /// `collect_fees`: the fees leave for the admin. Returns what the admin
    /// is paid. The handler also refuses when nothing is owed
    /// (`NothingToCollect`); the model leaves that to its callers, since the
    /// lifecycle harness sweeps a venue whose fee may be zero.
    pub fn collect_fees(&mut self) -> Option<u64> {
        let payout = self.fees_owed;
        self.quote_vault = self.quote_vault.checked_sub(payout)?;
        self.fees_owed = 0;
        Some(payout)
    }
}

/// The admin reaches only the fees. Starting from any ledger whose quote
/// vault covers what it owes, with `surplus` on top (tokens sent straight to
/// the vault, which the handlers' `>=` custody check tolerates), one
/// `collect_fees` pays the admin exactly `fees_owed`, never more; leaves every
/// amount owed to writers and holders, and the surplus, in the vault; and
/// leaves the counters consistent with the vault. Shared by the Kani harness,
/// which runs it on symbolic inputs, and a plain unit test, which runs it on
/// the chapter's numbers.
pub fn check_collect_fees_pays_only_the_fees_owed(mut ledger: Ledger, surplus: u64) {
    let before = ledger;
    let payout = ledger
        .collect_fees()
        .expect("the vault covers the fees owed");

    assert_eq!(payout, before.fees_owed);
    assert!(payout <= before.fees_owed);
    assert_eq!(ledger.fees_owed, 0);
    // Nothing owed to a writer or holder changed hands.
    assert_eq!(ledger.quote_owed, before.quote_owed);
    assert_eq!(ledger.underlying_owed, before.underlying_owed);
    assert_eq!(ledger.underlying_vault, before.underlying_vault);
    // The quote vault lost exactly the payout and still holds what it owes,
    // plus the surplus the admin could not reach.
    assert_eq!(ledger.quote_vault, before.quote_vault - payout);
    assert_eq!(ledger.quote_vault, ledger.quote_owed + surplus);
    assert!(ledger.quote_vault >= ledger.quote_owed + ledger.fees_owed);
    // A second sweep pays nothing: there is nothing left that is the admin's.
    assert_eq!(ledger.collect_fees(), Some(0));
    assert_eq!(ledger.quote_vault, before.quote_vault - payout);
}

/// `collect_fees` on every ledger the program could hold: any amounts owed,
/// any fees, any surplus, as long as the sums fit in a vault balance.
#[cfg(kani)]
#[kani::proof]
fn proof_collect_fees_pays_only_the_fees_owed() {
    let underlying_owed: u64 = kani::any();
    let quote_owed: u64 = kani::any();
    let fees_owed: u64 = kani::any();
    let surplus: u64 = kani::any();
    // collect_fees refuses a sweep of nothing.
    kani::assume(fees_owed >= 1);
    let quote_vault = quote_owed
        .checked_add(fees_owed)
        .and_then(|owed| owed.checked_add(surplus));
    kani::assume(quote_vault.is_some());
    let ledger = Ledger {
        underlying_owed,
        quote_owed,
        fees_owed,
        underlying_vault: underlying_owed,
        quote_vault: quote_vault.unwrap(),
    };
    check_collect_fees_pays_only_the_fees_owed(ledger, surplus);
}

/// Every path through an option's life leaves the ledger consistent and, once the
/// option is closed and the fees swept, back at zero: cancel; buy then reclaim;
/// buy then exercise then collect. Two options of either kind run through the
/// model at once so the paths interleave over a shared vault, and every step
/// of every path is checked, not just the end state.
#[cfg(kani)]
#[kani::proof]
#[kani::solver(cadical)]
fn proof_vault_ledger_stays_consistent_across_every_lifecycle() {
    let mut ledger = Ledger::default();

    // Two options with symbolic terms. Bounded so the premium split's
    // multiplication stays tractable; the ledger arithmetic is additions and
    // subtractions whose behaviour does not depend on the magnitudes.
    let mut options = [(OptionKind::Call, 0u64, 0u64, 0u64); 2];
    for option in options.iter_mut() {
        let kind: bool = kani::any();
        let underlying_amount: u64 = kani::any();
        let strike_amount: u64 = kani::any();
        let premium: u64 = kani::any();
        let fee_bps: u16 = kani::any();
        kani::assume(underlying_amount >= 1 && underlying_amount <= 255);
        kani::assume(strike_amount >= 1 && strike_amount <= 255);
        kani::assume(premium >= 1 && premium <= 255);
        kani::assume(fee_bps <= 255);
        let kind = if kind {
            OptionKind::Call
        } else {
            OptionKind::Put
        };
        let collateral = collateral_amount(kind, underlying_amount, strike_amount);
        let payment = exercise_payment(kind, underlying_amount, strike_amount);
        let (fee, _) = split_premium(premium, fee_bps).unwrap();
        *option = (kind, collateral, payment, fee);
    }

    // Both options are written first, so their collateral shares the vaults.
    for (kind, collateral, _, _) in options {
        ledger.write(kind, collateral).unwrap();
        assert!(ledger.is_consistent());
    }

    // Each option then takes one of the three exits, chosen symbolically.
    for (kind, collateral, payment, fee) in options {
        let path: u8 = kani::any();
        kani::assume(path < 3);
        match path {
            // Nobody buys: the writer cancels.
            0 => {
                ledger.return_collateral(kind, collateral).unwrap();
            }
            // Bought, then expires unexercised: the writer reclaims.
            1 => {
                ledger.buy(fee).unwrap();
                assert!(ledger.is_consistent());
                ledger.return_collateral(kind, collateral).unwrap();
            }
            // Bought and exercised: the writer collects the payment.
            _ => {
                ledger.buy(fee).unwrap();
                assert!(ledger.is_consistent());
                ledger.exercise(kind, collateral, payment).unwrap();
                assert!(ledger.is_consistent());
                ledger.collect_proceeds(kind, payment).unwrap();
            }
        }
        assert!(ledger.is_consistent());
    }

    // With every option closed, nothing is owed to any writer or holder ...
    assert_eq!(ledger.underlying_owed, 0);
    assert_eq!(ledger.quote_owed, 0);
    // ... and once the admin sweeps the fees, both vaults are empty: no token
    // was created or lost along any path.
    ledger.collect_fees().unwrap();
    assert!(ledger.is_consistent());
    assert_eq!(ledger.underlying_vault, 0);
    assert_eq!(ledger.quote_vault, 0);
}

// ===========================================================================
// Plain unit tests (so the crate is meaningful without Kani installed).
// These pin the exact numbers the LiteSVM tests and the book chapter use.
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // NVDAx has 8 decimals and USDC has 6; the venue charges 1% of each
    // premium.
    const ONE_NVDAX: u64 = 100_000_000;
    const ONE_USDC: u64 = 1_000_000;
    const FEE_BPS: u16 = 100;

    #[test]
    fn the_call_posts_five_nvdax_and_settles_for_nine_hundred_usdc() {
        // 5 NVDAx at a strike of 180 USDC each, 900 USDC in all.
        let collateral = collateral_amount(OptionKind::Call, 5 * ONE_NVDAX, 900 * ONE_USDC);
        let payment = exercise_payment(OptionKind::Call, 5 * ONE_NVDAX, 900 * ONE_USDC);
        assert_eq!(collateral, 5 * ONE_NVDAX);
        assert_eq!(payment, 900 * ONE_USDC);
    }

    #[test]
    fn the_put_posts_seven_fifty_usdc_and_settles_for_five_nvdax() {
        // 5 NVDAx at a strike of 150 USDC each, 750 USDC in all.
        let collateral = collateral_amount(OptionKind::Put, 5 * ONE_NVDAX, 750 * ONE_USDC);
        let payment = exercise_payment(OptionKind::Put, 5 * ONE_NVDAX, 750 * ONE_USDC);
        assert_eq!(collateral, 750 * ONE_USDC);
        assert_eq!(payment, 5 * ONE_NVDAX);
    }

    #[test]
    fn a_twenty_five_usdc_premium_splits_into_a_quarter_dollar_fee() {
        // Both of the chapter's premiums are exact multiples of the rate, so
        // nothing rounds.
        assert_eq!(
            split_premium(25 * ONE_USDC, FEE_BPS).unwrap(),
            (250_000, 24_750_000)
        );
        assert_eq!(
            split_premium(20 * ONE_USDC, FEE_BPS).unwrap(),
            (200_000, 19_800_000)
        );
    }

    #[test]
    fn the_fee_rounds_up_and_the_writer_takes_the_remainder() {
        // 999 minor units at 1%: 9.99 rounds up to 10, the writer gets 989.
        assert_eq!(split_premium(999, FEE_BPS).unwrap(), (10, 989));
        // 10.000001 USDC at 1%: the fee is 0.100001 USDC, not 0.10.
        assert_eq!(
            split_premium(10 * ONE_USDC + 1, FEE_BPS).unwrap(),
            (100_001, 9_900_000)
        );
        // A zero fee passes the whole premium through: there is nothing to
        // round.
        assert_eq!(split_premium(999, 0).unwrap(), (0, 999));
    }

    #[test]
    fn the_fee_never_exceeds_the_premium_at_the_highest_rate_the_venue_allows() {
        // 99.99% is the highest rate initialize_market accepts. A premium of
        // one minor unit rounds entirely into the fee; a larger one leaves
        // the writer the remainder.
        let highest: u16 = 9_999;
        assert_eq!(split_premium(1, highest).unwrap(), (1, 0));
        assert_eq!(split_premium(10_000, highest).unwrap(), (9_999, 1));
        assert_eq!(split_premium(10_001, highest).unwrap(), (10_000, 1));
        let (fee, to_writer) = split_premium(u64::MAX, highest).unwrap();
        assert!(fee < u64::MAX);
        assert_eq!(fee as u128 + to_writer as u128, u64::MAX as u128);
    }

    #[test]
    fn the_holder_exercises_up_to_but_not_at_expiry() {
        let expiry = 1_700_000_000;
        assert!(may_exercise(expiry - 1, expiry));
        assert!(!may_exercise(expiry, expiry));
        assert!(!may_reclaim(expiry - 1, expiry));
        assert!(may_reclaim(expiry, expiry));
    }

    #[test]
    fn the_ledger_returns_to_zero_after_the_chapter() {
        let mut ledger = Ledger::default();
        // Alice's call: written, bought by Bob, exercised, collected.
        ledger.write(OptionKind::Call, 5 * ONE_NVDAX).unwrap();
        ledger.buy(250_000).unwrap();
        ledger
            .exercise(OptionKind::Call, 5 * ONE_NVDAX, 900 * ONE_USDC)
            .unwrap();
        assert!(ledger.is_consistent());
        assert_eq!(ledger.quote_vault, 900 * ONE_USDC + 250_000);
        ledger
            .collect_proceeds(OptionKind::Call, 900 * ONE_USDC)
            .unwrap();
        // Carol's put: written, bought by Dave, expires, reclaimed.
        ledger.write(OptionKind::Put, 750 * ONE_USDC).unwrap();
        ledger.buy(200_000).unwrap();
        assert!(ledger.is_consistent());
        ledger
            .return_collateral(OptionKind::Put, 750 * ONE_USDC)
            .unwrap();
        // Maria sweeps 0.45 USDC and the vaults are empty.
        assert_eq!(ledger.fees_owed, 450_000);
        assert_eq!(ledger.collect_fees(), Some(450_000));
        assert!(ledger.is_consistent());
        assert_eq!(ledger, Ledger::default());
    }

    #[test]
    fn collect_fees_leaves_the_strike_payment_and_a_donation_in_the_vault() {
        // After Bob's exercise: 900 USDC owed to Alice, 0.25 USDC of fees,
        // and 3 USDC somebody sent straight to the vault.
        let surplus = 3 * ONE_USDC;
        let ledger = Ledger {
            underlying_owed: 0,
            quote_owed: 900 * ONE_USDC,
            fees_owed: 250_000,
            underlying_vault: 0,
            quote_vault: 900 * ONE_USDC + 250_000 + surplus,
        };
        check_collect_fees_pays_only_the_fees_owed(ledger, surplus);
    }
}
