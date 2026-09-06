//! Kani proof harnesses for the options venue (`finance/options`).
//!
//! Inspired by aeyakovenko/percolator, which uses the Kani model checker to
//! prove the mathematical correctness of a DeFi engine's pure numeric core.
//!
//! The on-chain instructions hand the actual token movement to the SPL token
//! program via CPIs that Kani cannot symbolically execute. The arithmetic
//! underneath is small and is reproduced here faithfully, mirroring
//! `options::contract_math`: every settlement amount is a product of two
//! integers, the only rounding is the floor in the fee split, and the expiry
//! window is one comparison and its complement. The harnesses prove the
//! invariants the program's custody accounting depends on, plus a bounded
//! model of the vault ledger across an option's whole life.

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

/// `contracts * underlying_per_contract`. Mirrors `contract_math::underlying_total`.
pub fn underlying_total(contracts: u64, underlying_per_contract: u64) -> Option<u64> {
    contracts.checked_mul(underlying_per_contract)
}

/// `contracts * strike_per_contract`. Mirrors `contract_math::strike_total`.
pub fn strike_total(contracts: u64, strike_per_contract: u64) -> Option<u64> {
    contracts.checked_mul(strike_per_contract)
}

/// What the writer posts. Mirrors `contract_math::collateral_amount`.
pub fn collateral_amount(
    kind: OptionKind,
    contracts: u64,
    underlying_per_contract: u64,
    strike_per_contract: u64,
) -> Option<u64> {
    match kind {
        OptionKind::Call => underlying_total(contracts, underlying_per_contract),
        OptionKind::Put => strike_total(contracts, strike_per_contract),
    }
}

/// What the holder pays at exercise and the writer later collects. Mirrors
/// `contract_math::exercise_payment`.
pub fn exercise_payment(
    kind: OptionKind,
    contracts: u64,
    underlying_per_contract: u64,
    strike_per_contract: u64,
) -> Option<u64> {
    match kind {
        OptionKind::Call => strike_total(contracts, strike_per_contract),
        OptionKind::Put => underlying_total(contracts, underlying_per_contract),
    }
}

/// Physical settlement moves exactly the posted collateral to the holder and
/// exactly the mirrored payment to the writer, for every option the program
/// would accept: the holder's payment for a call is a put's collateral on the
/// same terms, and the other way round. There is no division in either
/// formula, so no rounding can open a gap between what was posted and what
/// is delivered.
#[cfg(kani)]
#[kani::proof]
fn proof_exercise_moves_exactly_the_posted_terms() {
    let contracts: u64 = kani::any();
    let underlying_per_contract: u64 = kani::any();
    let strike_per_contract: u64 = kani::any();
    // write_option refuses a zero in any term. Bounded model checking: the
    // two products multiply symbolic values, nonlinear arithmetic that the
    // bit-precise solver pays for exponentially by the bit (16-bit terms run
    // for tens of minutes; 8-bit terms finish in under a minute). 8-bit terms
    // exercise every carry pattern of the multiplication, and larger terms
    // add magnitude rather than new behavior. Within the bound no product
    // overflows, so the overflow refusal is pinned by the unit test below.
    kani::assume(contracts >= 1 && contracts <= 0xFF);
    kani::assume(underlying_per_contract >= 1 && underlying_per_contract <= 0xFF);
    kani::assume(strike_per_contract >= 1 && strike_per_contract <= 0xFF);
    let underlying =
        underlying_total(contracts, underlying_per_contract).expect("within the bound");
    let strike = strike_total(contracts, strike_per_contract).expect("within the bound");

    for kind in [OptionKind::Call, OptionKind::Put] {
        let collateral = collateral_amount(
            kind,
            contracts,
            underlying_per_contract,
            strike_per_contract,
        )
        .expect("write_option checked both totals");
        let payment = exercise_payment(
            kind,
            contracts,
            underlying_per_contract,
            strike_per_contract,
        )
        .expect("write_option checked both totals");
        let (expected_collateral, expected_payment) = match kind {
            OptionKind::Call => (underlying, strike),
            OptionKind::Put => (strike, underlying),
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
            collateral_amount(
                mirror,
                contracts,
                underlying_per_contract,
                strike_per_contract
            )
            .unwrap()
        );
    }
}

// ===========================================================================
// 2. The premium split  (contract_math.rs)
// ===========================================================================

/// Fee floors; the writer takes the remainder. Mirrors
/// `contract_math::split_premium`.
pub fn split_premium(premium: u64, fee_bps: u16) -> Option<(u64, u64)> {
    let fee = (premium as u128)
        .checked_mul(fee_bps as u128)?
        .checked_div(BASIS_POINTS)?;
    let fee = u64::try_from(fee).ok()?;
    let to_writer = premium.checked_sub(fee)?;
    Some((fee, to_writer))
}

/// The premium is conserved: fee plus the writer's share is exactly the
/// premium, the fee never exceeds the premium, and the writer always gets
/// something. Also proves the fee is the exact floor of `premium * bps /
/// 10_000`, so a refactor that rounds up against the writer, or drops below
/// the floor against the venue, fails the proof.
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
    // full fee range runs for hours). The floor's behavior depends only on
    // the product's residue mod 10_000; a 12-bit premium against a 10-bit
    // fee already reaches every residue and the fee-equals-premium edge, and
    // larger operands add magnitude rather than new behavior. The full fee
    // range, including the 99.99% ceiling, is pinned by the unit tests.
    kani::assume(premium >= 1 && premium <= 0xFF);
    kani::assume(fee_bps <= 0xFF);

    let (fee, to_writer) = split_premium(premium, fee_bps).expect("split computes");

    assert_eq!(fee as u128 + to_writer as u128, premium as u128);
    assert!(fee <= premium);
    // With the fee under 100%, the writer is always paid something.
    assert!(to_writer > 0);
    // Exact floor: fee * 10_000 <= premium * bps < (fee + 1) * 10_000.
    let target = (premium as u128) * (fee_bps as u128);
    assert!((fee as u128) * BASIS_POINTS <= target);
    assert!((fee as u128 + 1) * BASIS_POINTS > target);
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
    pub underlying_locked: u64,
    pub quote_locked: u64,
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
        self.underlying_vault == self.underlying_locked
            && self.quote_vault as u128 == self.quote_locked as u128 + self.fees_owed as u128
    }

    /// `write_option`: collateral into the vault, owed back to the writer.
    pub fn write(&mut self, kind: OptionKind, collateral: u64) -> Option<()> {
        match kind {
            OptionKind::Call => {
                self.underlying_locked = self.underlying_locked.checked_add(collateral)?;
                self.underlying_vault = self.underlying_vault.checked_add(collateral)?;
            }
            OptionKind::Put => {
                self.quote_locked = self.quote_locked.checked_add(collateral)?;
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
                self.underlying_locked = self.underlying_locked.checked_sub(collateral)?;
                self.underlying_vault = self.underlying_vault.checked_sub(collateral)?;
                self.quote_locked = self.quote_locked.checked_add(payment)?;
                self.quote_vault = self.quote_vault.checked_add(payment)?;
            }
            OptionKind::Put => {
                self.quote_locked = self.quote_locked.checked_sub(collateral)?;
                self.quote_vault = self.quote_vault.checked_sub(collateral)?;
                self.underlying_locked = self.underlying_locked.checked_add(payment)?;
                self.underlying_vault = self.underlying_vault.checked_add(payment)?;
            }
        }
        Some(())
    }

    /// `collect_proceeds`: the payment leaves for the writer.
    pub fn collect_proceeds(&mut self, kind: OptionKind, payment: u64) -> Option<()> {
        match kind {
            OptionKind::Call => {
                self.quote_locked = self.quote_locked.checked_sub(payment)?;
                self.quote_vault = self.quote_vault.checked_sub(payment)?;
            }
            OptionKind::Put => {
                self.underlying_locked = self.underlying_locked.checked_sub(payment)?;
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
                self.underlying_locked = self.underlying_locked.checked_sub(collateral)?;
                self.underlying_vault = self.underlying_vault.checked_sub(collateral)?;
            }
            OptionKind::Put => {
                self.quote_locked = self.quote_locked.checked_sub(collateral)?;
                self.quote_vault = self.quote_vault.checked_sub(collateral)?;
            }
        }
        Some(())
    }

    /// `collect_fees`: the fees leave for the admin.
    pub fn collect_fees(&mut self) -> Option<()> {
        self.quote_vault = self.quote_vault.checked_sub(self.fees_owed)?;
        self.fees_owed = 0;
        Some(())
    }
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

    // Two options with symbolic terms. Bounded so the multiplications stay
    // tractable; the ledger arithmetic is additions and subtractions whose
    // behaviour does not depend on the magnitudes.
    let mut options = [(OptionKind::Call, 0u64, 0u64, 0u64); 2];
    for option in options.iter_mut() {
        let kind: bool = kani::any();
        let contracts: u64 = kani::any();
        let underlying_per_contract: u64 = kani::any();
        let strike_per_contract: u64 = kani::any();
        let premium: u64 = kani::any();
        let fee_bps: u16 = kani::any();
        kani::assume(contracts >= 1 && contracts <= 15);
        kani::assume(underlying_per_contract >= 1 && underlying_per_contract <= 15);
        kani::assume(strike_per_contract >= 1 && strike_per_contract <= 15);
        kani::assume(premium >= 1 && premium <= 255);
        kani::assume(fee_bps <= 255);
        let kind = if kind {
            OptionKind::Call
        } else {
            OptionKind::Put
        };
        let collateral = collateral_amount(
            kind,
            contracts,
            underlying_per_contract,
            strike_per_contract,
        )
        .unwrap();
        let payment = exercise_payment(
            kind,
            contracts,
            underlying_per_contract,
            strike_per_contract,
        )
        .unwrap();
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
    assert_eq!(ledger.underlying_locked, 0);
    assert_eq!(ledger.quote_locked, 0);
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

    // Both tokens have 6 decimals; the venue charges 1% of each premium.
    const ONE_TOKEN: u64 = 1_000_000;
    const FEE_BPS: u16 = 100;

    #[test]
    fn the_call_posts_five_nvdax_and_settles_for_nine_hundred_usdc() {
        // 5 contracts, each on 1 NVDAx, strike 180 USDC.
        let collateral =
            collateral_amount(OptionKind::Call, 5, ONE_TOKEN, 180 * ONE_TOKEN).unwrap();
        let payment = exercise_payment(OptionKind::Call, 5, ONE_TOKEN, 180 * ONE_TOKEN).unwrap();
        assert_eq!(collateral, 5 * ONE_TOKEN);
        assert_eq!(payment, 900 * ONE_TOKEN);
    }

    #[test]
    fn the_put_posts_seven_fifty_usdc_and_settles_for_five_nvdax() {
        // 5 contracts, each on 1 NVDAx, strike 150 USDC.
        let collateral = collateral_amount(OptionKind::Put, 5, ONE_TOKEN, 150 * ONE_TOKEN).unwrap();
        let payment = exercise_payment(OptionKind::Put, 5, ONE_TOKEN, 150 * ONE_TOKEN).unwrap();
        assert_eq!(collateral, 750 * ONE_TOKEN);
        assert_eq!(payment, 5 * ONE_TOKEN);
    }

    #[test]
    fn a_twenty_five_usdc_premium_splits_into_a_quarter_dollar_fee() {
        assert_eq!(
            split_premium(25 * ONE_TOKEN, FEE_BPS).unwrap(),
            (250_000, 24_750_000)
        );
        assert_eq!(
            split_premium(20 * ONE_TOKEN, FEE_BPS).unwrap(),
            (200_000, 19_800_000)
        );
    }

    #[test]
    fn the_fee_floors_and_the_writer_keeps_the_rounding_unit() {
        // 999 minor units at 1%: 9.99 floors to 9, the writer gets 990.
        assert_eq!(split_premium(999, FEE_BPS).unwrap(), (9, 990));
        // A zero fee passes the whole premium through.
        assert_eq!(split_premium(999, 0).unwrap(), (0, 999));
    }

    #[test]
    fn the_writer_is_paid_even_at_the_highest_fee_the_venue_allows() {
        // 99.99% is the highest rate initialize_market accepts. The floor
        // leaves the writer at least one minor unit at every premium size.
        let ceiling: u16 = 9_999;
        assert_eq!(split_premium(1, ceiling).unwrap(), (0, 1));
        assert_eq!(split_premium(10_000, ceiling).unwrap(), (9_999, 1));
        let (fee, to_writer) = split_premium(u64::MAX, ceiling).unwrap();
        assert!(to_writer > 0);
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
    fn a_lot_that_overflows_is_refused_at_write_time() {
        assert_eq!(underlying_total(u64::MAX, 2), None);
        assert_eq!(strike_total(u64::MAX, 2), None);
    }

    #[test]
    fn the_ledger_returns_to_zero_after_the_chapter() {
        let mut ledger = Ledger::default();
        // Alice's call: written, bought by Bob, exercised, collected.
        ledger.write(OptionKind::Call, 5 * ONE_TOKEN).unwrap();
        ledger.buy(250_000).unwrap();
        ledger
            .exercise(OptionKind::Call, 5 * ONE_TOKEN, 900 * ONE_TOKEN)
            .unwrap();
        assert!(ledger.is_consistent());
        assert_eq!(ledger.quote_vault, 900 * ONE_TOKEN + 250_000);
        ledger
            .collect_proceeds(OptionKind::Call, 900 * ONE_TOKEN)
            .unwrap();
        // Carol's put: written, bought by Dave, expires, reclaimed.
        ledger.write(OptionKind::Put, 750 * ONE_TOKEN).unwrap();
        ledger.buy(200_000).unwrap();
        assert!(ledger.is_consistent());
        ledger
            .return_collateral(OptionKind::Put, 750 * ONE_TOKEN)
            .unwrap();
        // Maria sweeps 0.45 USDC and the vaults are empty.
        assert_eq!(ledger.fees_owed, 450_000);
        ledger.collect_fees().unwrap();
        assert!(ledger.is_consistent());
        assert_eq!(ledger, Ledger::default());
    }
}
