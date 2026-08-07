//! Kani proof harnesses for the lending program (`finance/lending`).
//!
//! Inspired by aeyakovenko/percolator, which uses the Kani model checker to
//! prove the mathematical correctness of a DeFi engine's pure numeric core.
//!
//! The lending program is the richest of the finance examples: a Solend-style
//! pool with `mul_div` floor/ceil rounding (`math.rs`), a kinked interest-rate
//! curve and a compounding accumulation factor (`state::reserve`), a share-token
//! exchange rate (`deposit`/`redeem`), and liquidation sizing with a close
//! factor and bonus (`liquidate_obligation`). All of that is pure integer
//! arithmetic; the token movement is delegated to SPL CPIs that Kani cannot
//! symbolically execute. This crate reproduces the formulas faithfully and
//! proves the invariants the protocol's safety rests on.
//!
//! Nonlinear 128-bit arithmetic is the hard case for a bit-precise solver, so —
//! as percolator does — the harnesses use bounded model checking: symbolic
//! inputs are capped to a representative range (the identities are
//! scale-invariant, so every rounding boundary is still exercised). Where the
//! real code uses `FIXED_POINT_SCALE = 10^18`, the *scale-invariant* harnesses
//! use a small symbolic scale instead, because the property ("the index only
//! grows", "rounding never extracts value") holds for any scale and a 10^18
//! constant would make the solver intractable.

#![cfg_attr(kani, allow(dead_code))]

/// `constants::BPS_DENOMINATOR`.
pub const BPS_DENOMINATOR: u128 = 10_000;

// ===========================================================================
// 1. mul_div floor / ceil  (math.rs)
// ===========================================================================

/// `floor((a*b)/d)`, `None` on overflow / zero divisor (mirrors `mul_div_floor`).
pub fn mul_div_floor(a: u128, b: u128, d: u128) -> Option<u128> {
    if d == 0 {
        return None;
    }
    a.checked_mul(b)?.checked_div(d)
}

/// `ceil((a*b)/d)`, computed as `(a*b + (d-1)) / d` (mirrors `mul_div_ceil`).
pub fn mul_div_ceil(a: u128, b: u128, d: u128) -> Option<u128> {
    if d == 0 {
        return None;
    }
    let product = a.checked_mul(b)?;
    product.checked_add(d.checked_sub(1)?)?.checked_div(d)
}

/// The two rounding helpers are correct and consistent: floor is the greatest
/// integer with `floor*d <= a*b`, ceil is the least integer with `ceil*d >= a*b`,
/// they differ by at most one, and they coincide exactly when `d | a*b`.
#[cfg(kani)]
#[kani::proof]
#[kani::solver(cadical)]
fn proof_mul_div_floor_ceil_correct() {
    let a: u128 = kani::any();
    let b: u128 = kani::any();
    let d: u128 = kani::any();
    // Bounded model checking: cap the symbolic factors so `a*b` stays small; the
    // divisor `d` is symbolic, and each `*d` / `/d` against a symbolic divisor is
    // expensive for the bit-precise solver, so keep the bound tight.
    kani::assume(a <= 31 && b <= 31);
    kani::assume(d >= 1 && d <= 31);

    let product = a * b;
    let floor = mul_div_floor(a, b, d).unwrap();
    let ceil = mul_div_ceil(a, b, d).unwrap();

    // floor correctness: floor*d <= product < (floor+1)*d
    assert!(floor * d <= product);
    assert!(product < (floor + 1) * d);
    // ceil correctness: ceil is the least integer with ceil*d >= product
    assert!(ceil * d >= product);
    assert!(ceil == 0 || (ceil - 1) * d < product);
    // relationship: ceil and floor differ by at most one, ceil never below floor.
    assert!(ceil >= floor);
    assert!(ceil - floor <= 1);
    // They coincide exactly when the division is exact (floor*d recovers the
    // product) - expressed via the already-computed `floor` to avoid a second
    // symbolic `% d`.
    assert_eq!(ceil == floor, floor * d == product);
}

/// Directional-rounding safety, the property `math.rs`'s `Rounding` enum exists
/// to guarantee: debt/protocol quantities (rounded UP) are never *less* than the
/// same quantity rounded DOWN for the user. So a borrower's debt is never
/// undercounted and a supplier's claim is never overcounted by rounding — the
/// protocol cannot be drained by repeated round-trips.
#[cfg(kani)]
#[kani::proof]
#[kani::solver(cadical)]
fn proof_rounding_is_protocol_favourable() {
    let a: u128 = kani::any();
    let b: u128 = kani::any();
    let d: u128 = kani::any();
    kani::assume(a <= 127 && b <= 127);
    kani::assume(d >= 1 && d <= 127);

    let up = mul_div_ceil(a, b, d).unwrap(); // debt / protocol-owed
    let down = mul_div_floor(a, b, d).unwrap(); // user-favourable
    assert!(up >= down);
}

// ===========================================================================
// 2. Compounding accumulation factor  (reserve::accrue_interest)
// ===========================================================================

/// Factor update `new = floor(old * growth / scale)` where the growth per
/// accrual is `scale + accrued` (so always `>= scale`). Generic in `scale`
/// because the property is scale-invariant (the real code uses
/// `FIXED_POINT_SCALE = 10^18`).
pub fn grow_factor(old_factor: u128, accrued: u128, scale: u128) -> Option<u128> {
    let growth = scale.checked_add(accrued)?;
    mul_div_floor(old_factor, growth, scale)
}

/// The borrow accumulation factor is monotonically non-decreasing: each
/// accrual multiplies by a factor `>= 1`, so `new_factor >= old_factor`. A debt
/// scaled by this value can therefore never shrink from interest accrual, the
/// core guarantee that borrowers always owe at least their principal.
#[cfg(kani)]
#[kani::proof]
#[kani::solver(cadical)]
fn proof_accumulation_factor_monotonic() {
    let old_factor: u128 = kani::any();
    let accrued: u128 = kani::any();
    let scale: u128 = kani::any();
    // Bounded model checking with a small symbolic scale (the 10^18 real scale
    // is scale-invariant for this property and would be intractable).
    kani::assume(scale >= 1 && scale <= 127);
    kani::assume(old_factor <= 255);
    kani::assume(accrued <= 255);

    let new_factor = grow_factor(old_factor, accrued, scale).unwrap();
    assert!(new_factor >= old_factor); // the factor never decreases
}

// ===========================================================================
// 3. Utilization and the kinked borrow-rate curve  (reserve.rs)
// ===========================================================================

/// `utilization_bps = floor(borrowed * 10_000 / gross)` (0 if the pool is empty).
pub fn utilization_bps(borrowed: u128, gross: u128) -> u128 {
    if gross == 0 {
        return 0;
    }
    mul_div_floor(borrowed, BPS_DENOMINATOR, gross).unwrap()
}

/// Utilization is always a valid fraction in `[0, 10_000]` bps, because the
/// borrowed amount can never exceed gross liquidity (`gross = available +
/// borrowed`). Keeps the rate curve's domain well-defined.
#[cfg(kani)]
#[kani::proof]
#[kani::solver(cadical)]
fn proof_utilization_in_range() {
    let borrowed: u128 = kani::any();
    let gross: u128 = kani::any();
    kani::assume(gross <= 4095);
    kani::assume(borrowed <= gross); // borrowed is a subset of gross liquidity

    let util = utilization_bps(borrowed, gross);
    assert!(util <= BPS_DENOMINATOR);
}

/// The kinked borrow-rate APR (bps) from `current_borrow_rate_per_slot`, given a
/// utilization and the curve parameters. Mirrors the two-segment formula.
///
/// `full_utilization` is the 100%-utilization denominator — `BPS_DENOMINATOR`
/// (10_000) on-chain. It is a parameter here only so the scale-invariant
/// in-bounds proof can use a small denominator: dividing by a symbolic value
/// near 10_000 is intractable for the bit-precise solver, but the property is
/// identical at any scale.
pub fn borrow_rate_bps(
    utilization: u128,
    min_rate: u128,
    optimal_rate: u128,
    max_rate: u128,
    optimal_utilization: u128,
    full_utilization: u128,
) -> Option<u128> {
    if utilization <= optimal_utilization {
        let rate_range = optimal_rate.checked_sub(min_rate)?;
        let climbed = mul_div_floor(rate_range, utilization, optimal_utilization)?;
        min_rate.checked_add(climbed)
    } else {
        let rate_range = max_rate.checked_sub(optimal_rate)?;
        let utilization_above = utilization.checked_sub(optimal_utilization)?;
        let utilization_range = full_utilization.checked_sub(optimal_utilization)?;
        let climbed = mul_div_floor(rate_range, utilization_above, utilization_range)?;
        optimal_rate.checked_add(climbed)
    }
}

/// The borrow-rate curve stays within `[min_rate, max_rate]` for every
/// utilization in `[0, 10_000]`, given the config ordering the validator
/// enforces (`min <= optimal <= max`, `0 < optimal_utilization < 10_000`). So
/// the interest rate can never escape its configured bounds regardless of pool
/// state — no utilization makes a borrower pay below `min` or above `max`.
#[cfg(kani)]
#[kani::proof]
#[kani::solver(cadical)]
fn proof_borrow_rate_within_bounds() {
    let utilization: u128 = kani::any();
    let min_rate: u128 = kani::any();
    let optimal_rate: u128 = kani::any();
    let max_rate: u128 = kani::any();
    let optimal_utilization: u128 = kani::any();
    // Scale-invariant 100%-utilization denominator (10_000 on-chain), kept small
    // and symbolic here so the `/ (full - optimal)` divisor stays tractable.
    let full_utilization: u128 = kani::any();

    kani::assume(full_utilization >= 2 && full_utilization <= 32);
    // ReserveConfig::validate guarantees:
    kani::assume(min_rate <= optimal_rate && optimal_rate <= max_rate);
    kani::assume(optimal_utilization >= 1 && optimal_utilization < full_utilization);
    kani::assume(utilization <= full_utilization);
    // Bounded model checking: cap the rate magnitudes (they are u16 bps on-chain).
    kani::assume(max_rate <= 255);

    let apr = borrow_rate_bps(
        utilization,
        min_rate,
        optimal_rate,
        max_rate,
        optimal_utilization,
        full_utilization,
    )
    .expect("rate computes for valid config");
    assert!(apr >= min_rate);
    assert!(apr <= max_rate);
}

// ===========================================================================
// 4. Share exchange rate  (deposit / redeem)
// ===========================================================================

/// Shares minted for a deposit: `floor(amount * supply / total_liquidity)`.
pub fn deposit_to_shares(amount: u128, supply: u128, total_liquidity: u128) -> Option<u128> {
    mul_div_floor(amount, supply, total_liquidity)
}

/// Liquidity returned for a redeem: `floor(shares * total_liquidity / supply)`.
pub fn shares_to_liquidity(shares: u128, total_liquidity: u128, supply: u128) -> Option<u128> {
    mul_div_floor(shares, total_liquidity, supply)
}

/// A deposit-then-redeem round-trip can never return more liquidity than was put
/// in. Both legs floor (in the protocol's favour), so redeeming the shares a
/// deposit minted yields `<= amount` — there is no rounding round-trip that
/// extracts value from the pool. This is the supplier-side analogue of the AMM's
/// constant-product safety.
#[cfg(kani)]
#[kani::proof]
#[kani::solver(cadical)]
fn proof_deposit_redeem_cannot_extract() {
    let amount: u128 = kani::any();
    let supply: u128 = kani::any();
    let total_liquidity: u128 = kani::any();
    // Bounded model checking; an established pool has positive supply/liquidity.
    kani::assume(amount <= 31);
    kani::assume(supply >= 1 && supply <= 31);
    kani::assume(total_liquidity >= 1 && total_liquidity <= 31);

    let shares = deposit_to_shares(amount, supply, total_liquidity).unwrap();
    let back = shares_to_liquidity(shares, total_liquidity, supply).unwrap();
    assert!(back <= amount); // rounding never pays out more than deposited
}

// ===========================================================================
// 5. Liquidation sizing  (liquidate_obligation)
// ===========================================================================

/// A single liquidation can never repay more than the outstanding debt, because
/// the close factor (`<= 10_000` bps) caps `max_repay = floor(debt * cf / 10_000)
/// <= debt`, and the actual repay is `min(requested, max_repay)`. So a
/// liquidator can never over-repay and over-seize on the debt side.
#[cfg(kani)]
#[kani::proof]
#[kani::solver(cadical)]
fn proof_liquidation_repay_bounded_by_debt() {
    let debt: u64 = kani::any();
    let close_factor_bps: u16 = kani::any();
    let requested: u64 = kani::any();

    // Bounded model checking; validate() enforces close_factor in (0, 10_000].
    kani::assume(debt as u128 <= 4095);
    kani::assume(close_factor_bps >= 1 && (close_factor_bps as u128) <= BPS_DENOMINATOR);

    let max_repay = mul_div_floor(debt as u128, close_factor_bps as u128, BPS_DENOMINATOR).unwrap();
    assert!(max_repay <= debt as u128); // close factor never exceeds 100%

    let repay = (requested as u128).min(max_repay);
    assert!(repay <= debt as u128);
}

/// The seized collateral value always includes the liquidation bonus on top of
/// the repaid value (`seize_value = repay_value + floor(repay_value * bonus /
/// 10_000) >= repay_value`), so a liquidator is always compensated at least the
/// value they repaid — never less. The bonus addition also never overflows for
/// in-range values.
#[cfg(kani)]
#[kani::proof]
#[kani::solver(cadical)]
fn proof_seize_value_includes_bonus() {
    let repay_value: u128 = kani::any();
    let bonus_bps: u128 = kani::any();
    kani::assume(repay_value <= 4095);
    kani::assume(bonus_bps <= BPS_DENOMINATOR); // validate(): bonus <= 10_000 bps

    let bonus_value = mul_div_floor(repay_value, bonus_bps, BPS_DENOMINATOR).unwrap();
    let seize_value = repay_value.checked_add(bonus_value).unwrap();
    assert!(seize_value >= repay_value); // liquidator never under-compensated
}

// ===========================================================================
// Plain unit tests (meaningful without Kani installed).
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn floor_ceil() {
        assert_eq!(mul_div_floor(7, 3, 2).unwrap(), 10); // 21/2 = 10.5 -> 10
        assert_eq!(mul_div_ceil(7, 3, 2).unwrap(), 11); // -> 11
        assert_eq!(mul_div_floor(6, 2, 3).unwrap(), 4); // exact
        assert_eq!(mul_div_ceil(6, 2, 3).unwrap(), 4); // exact -> equal
    }

    #[test]
    fn factor_grows() {
        // scale 100, old 150 (=1.5), accrued 10 (=0.1) -> 150*110/100 = 165.
        assert_eq!(grow_factor(150, 10, 100).unwrap(), 165);
        // zero accrual leaves the index unchanged.
        assert_eq!(grow_factor(150, 0, 100).unwrap(), 150);
    }

    #[test]
    fn rate_curve_endpoints() {
        // min 100, optimal 300, max 2000, kink at 8000 bps.
        // util 0 -> min; util 8000 -> optimal; util 10000 -> max.
        assert_eq!(borrow_rate_bps(0, 100, 300, 2000, 8000, 10000).unwrap(), 100);
        assert_eq!(borrow_rate_bps(8000, 100, 300, 2000, 8000, 10000).unwrap(), 300);
        assert_eq!(borrow_rate_bps(10000, 100, 300, 2000, 8000, 10000).unwrap(), 2000);
    }

    #[test]
    fn round_trip_no_extraction() {
        let shares = deposit_to_shares(100, 50, 70).unwrap();
        let back = shares_to_liquidity(shares, 70, 50).unwrap();
        assert!(back <= 100);
    }
}
