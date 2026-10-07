mod common;

use lending::errors::LendingError;

use common::{default_config, Env};
use lending::constants::{BORROW_RATE_CEILING_BPS, FIXED_POINT_SCALE};
use lending::state::Reserve;

#[test]
fn init_market_and_reserve() {
    let mut env = Env::new();
    let usdc = env.add_empty_reserve(6, common::dollars(1), default_config());

    let reserve = env.reserve(&usdc);
    assert_eq!(reserve.lending_market, env.market);
    assert_eq!(reserve.liquidity_mint, usdc.mint);
    assert_eq!(reserve.liquidity_decimals, 6);
    assert_eq!(reserve.available_liquidity, 0);
    assert_eq!(reserve.share_mint_supply, 0);
    assert_eq!(reserve.borrowed_principal, 0);
    // The accumulation factor starts at 1.0.
    assert_eq!(reserve.borrow_accumulation_factor, FIXED_POINT_SCALE);
}

#[test]
fn rejects_ltv_above_liquidation_threshold() {
    let mut env = Env::new();
    let usdc = env.add_reserve(6, common::dollars(1), default_config());

    let mut bad = default_config();
    bad.loan_to_value_bps = 9_000;
    bad.liquidation_threshold_bps = 8_000;
    let result = env.try_update_config(&usdc, bad);
    common::assert_program_error!(result, LendingError::InvalidConfig);
}

#[test]
fn rejects_misordered_interest_rate_curve() {
    let mut env = Env::new();
    let usdc = env.add_reserve(6, common::dollars(1), default_config());

    let mut bad = default_config();
    bad.min_borrow_rate_bps = 5_000;
    bad.optimal_borrow_rate_bps = 2_000; // optimal below min
    bad.max_borrow_rate_bps = 15_000;
    let result = env.try_update_config(&usdc, bad);
    common::assert_program_error!(result, LendingError::InvalidConfig);
}

/// The confidence limit is a fraction of the price, so it cannot exceed 100%.
#[test]
fn rejects_confidence_limit_wider_than_the_price() {
    let mut env = Env::new();
    let usdc = env.add_reserve(6, common::dollars(1), default_config());

    let mut bad = default_config();
    bad.max_confidence_bps = 10_001;
    let result = env.try_update_config(&usdc, bad);
    // A confidence limit above 100% of the price must be rejected.
    common::assert_program_error!(result, LendingError::InvalidConfig);

    let mut widest = default_config();
    widest.max_confidence_bps = 10_000;
    env.try_update_config(&usdc, widest).unwrap();
    assert_eq!(env.reserve(&usdc).config.max_confidence_bps, 10_000);
}

/// A zero limit admits only a band of zero, which no live feed reports, so
/// the reserve could never be valued: the config is rejected rather than
/// freezing every obligation that holds the asset.
#[test]
fn rejects_zero_confidence_limit() {
    let mut env = Env::new();
    let usdc = env.add_reserve(6, common::dollars(1), default_config());

    let mut bad = default_config();
    bad.max_confidence_bps = 0;
    let result = env.try_update_config(&usdc, bad);
    // A zero confidence limit must be rejected.
    common::assert_program_error!(result, LendingError::InvalidConfig);

    let mut tightest = default_config();
    tightest.max_confidence_bps = 1;
    env.try_update_config(&usdc, tightest).unwrap();
    assert_eq!(env.reserve(&usdc).config.max_confidence_bps, 1);
}

#[test]
fn accepts_valid_config_update() {
    let mut env = Env::new();
    let usdc = env.add_reserve(6, common::dollars(1), default_config());

    let mut updated = default_config();
    updated.loan_to_value_bps = 7_800;
    env.try_update_config(&usdc, updated).unwrap();
    assert_eq!(env.reserve(&usdc).config.loan_to_value_bps, 7_800);
}

/// A reserve at 50% utilization with a borrower drawing half the pool, so the
/// kinked curve resolves to a non-zero rate. Returns the collateral and borrow
/// reserves.
fn half_borrowed_reserve(env: &mut Env) -> (common::ReserveHandle, common::ReserveHandle) {
    let collateral = env.add_reserve(6, common::dollars(1), default_config());
    let borrow = env.add_reserve(6, common::dollars(1), default_config());

    let supplier = env.create_user();
    env.fund(&supplier, borrow.mint, 1_000_000_000);
    env.supply(&supplier, &borrow, 1_000_000_000);

    let borrower = env.create_user();
    env.fund(&borrower, collateral.mint, 1_000_000_000);
    env.fund(&borrower, borrow.mint, 0);
    env.supply(&borrower, &collateral, 1_000_000_000);
    let obligation = env.initialize_obligation(&borrower);
    env.post_collateral(&borrower, obligation, &collateral, 1_000_000_000);
    env.try_borrow(
        &borrower,
        obligation,
        &[&collateral],
        &[],
        &borrow,
        500_000_000,
    )
    .unwrap();
    (collateral, borrow)
}

/// The factor after one refresh `seconds` after the last: one multiply by
/// `1 + rate_per_second * seconds`, floored, exactly as the program does it.
fn factor_after(reserve: &Reserve, seconds: u128) -> u128 {
    let rate = reserve.current_borrow_rate_per_second().unwrap();
    reserve.borrow_accumulation_factor * (FIXED_POINT_SCALE + rate * seconds) / FIXED_POINT_SCALE
}

/// The rate fields are annual, and a year is a length of wall-clock time, so
/// interest accrues for the seconds on the Clock's timestamp and ignores the
/// slot count. Slots passing on their own accrue nothing; seconds passing on
/// their own accrue exactly the per-second rate times the seconds. A shorter
/// or longer slot therefore cannot change what a borrower pays.
#[test]
fn interest_accrues_by_seconds_not_slots() {
    let mut env = Env::new();
    let (_collateral, borrow) = half_borrowed_reserve(&mut env);
    let refresher = env.create_user();
    let before = env.reserve(&borrow);

    env.warp_slots(1_000_000);
    env.refresh_reserve_only(&refresher, &borrow);
    assert_eq!(
        env.reserve(&borrow).borrow_accumulation_factor,
        before.borrow_accumulation_factor,
        "a million slots with the clock standing still must accrue nothing"
    );

    let seconds = common::TENTH_OF_A_YEAR;
    env.shift_timestamp(seconds);
    env.refresh_reserve_only(&refresher, &borrow);
    let after = env.reserve(&borrow);
    assert!(after.borrow_accumulation_factor > before.borrow_accumulation_factor);
    assert_eq!(
        after.borrow_accumulation_factor,
        factor_after(&before, seconds as u128)
    );
    assert_eq!(after.last_accrual_timestamp, env.current_timestamp());
}

/// The leader writes the timestamp, and a timestamp at or before the stored
/// one accrues nothing and leaves the stored stamp alone. When the clock
/// moves forward again, only the seconds past the stored stamp are charged,
/// so no second is charged twice.
#[test]
fn a_timestamp_behind_the_last_accrual_charges_nothing() {
    let mut env = Env::new();
    let (_collateral, borrow) = half_borrowed_reserve(&mut env);
    let refresher = env.create_user();
    let before = env.reserve(&borrow);

    env.shift_timestamp(-600);
    env.refresh_reserve_only(&refresher, &borrow);
    let behind = env.reserve(&borrow);
    assert_eq!(
        behind.borrow_accumulation_factor,
        before.borrow_accumulation_factor
    );
    assert_eq!(behind.last_accrual_timestamp, before.last_accrual_timestamp);

    // 1,600 seconds forward from the shifted clock is 1,000 past the stamp.
    env.shift_timestamp(1_600);
    env.refresh_reserve_only(&refresher, &borrow);
    assert_eq!(
        env.reserve(&borrow).borrow_accumulation_factor,
        factor_after(&before, 1_000)
    );
}

/// Changing the rate curve accrues at the old curve first, so the seconds
/// since the last refresh are charged at the rates that applied to them
/// rather than repriced by the new ones.
#[test]
fn a_config_update_accrues_at_the_old_rates_first() {
    let mut env = Env::new();
    let (_collateral, borrow) = half_borrowed_reserve(&mut env);
    let before = env.reserve(&borrow);

    let seconds = common::TENTH_OF_A_YEAR;
    env.warp_seconds(seconds);
    let mut steeper = default_config();
    steeper.min_borrow_rate_bps = 1_000;
    steeper.optimal_borrow_rate_bps = 5_000;
    steeper.max_borrow_rate_bps = 20_000;
    env.try_update_config(&borrow, steeper).unwrap();

    let after = env.reserve(&borrow);
    assert_eq!(after.config.optimal_borrow_rate_bps, 5_000);
    assert_eq!(
        after.borrow_accumulation_factor,
        factor_after(&before, seconds as u128)
    );
}

/// A config whose rate curve is `min`/`optimal`/`max`, every other field the
/// default.
fn config_with_curve(min: u16, optimal: u16, max: u16) -> lending::state::ReserveConfig {
    let mut config = default_config();
    config.min_borrow_rate_bps = min;
    config.optimal_borrow_rate_bps = optimal;
    config.max_borrow_rate_bps = max;
    config
}

/// A reserve cannot be created with a rate above the 300% a year ceiling.
#[test]
fn rejects_borrow_rate_above_ceiling_at_initialize() {
    let mut env = Env::new();
    let owner = env.owner.insecure_clone();
    let market = env.market;
    let above = config_with_curve(200, 2_000, BORROW_RATE_CEILING_BPS + 1);
    let result = env.try_add_reserve_to(&owner, market, 6, common::dollars(1), above);
    common::assert_program_error!(result.map(|_| ()), LendingError::BorrowRateAboveCeiling);
}

/// No rate field may be raised above the ceiling on a live reserve. Each of
/// `min`, `optimal` and `max` is tried one past it on its own, with the other
/// two inside the ceiling. For `min` and `optimal` the curve is then misordered
/// as well, and the ceiling check runs first, so each case returns
/// `BorrowRateAboveCeiling` only through its own field's clause.
#[test]
fn rejects_borrow_rate_above_ceiling_on_update() {
    let mut env = Env::new();
    let (_collateral, borrow) = half_borrowed_reserve(&mut env);
    let above = BORROW_RATE_CEILING_BPS + 1;
    for curve in [
        (above, 2_000, 15_000),
        (200, above, 15_000),
        (200, 2_000, above),
    ] {
        let result = env.try_update_config(&borrow, config_with_curve(curve.0, curve.1, curve.2));
        common::assert_program_error!(result, LendingError::BorrowRateAboveCeiling);
    }
    let config = env.reserve(&borrow).config;
    assert_eq!(config.min_borrow_rate_bps, 200);
    assert_eq!(config.optimal_borrow_rate_bps, 2_000);
    assert_eq!(config.max_borrow_rate_bps, 15_000);
}

/// A rate exactly at the ceiling is accepted, at creation and on update.
#[test]
fn accepts_borrow_rate_at_ceiling() {
    let mut env = Env::new();
    let at_ceiling = config_with_curve(200, 2_000, BORROW_RATE_CEILING_BPS);
    let usdc = env.add_reserve(6, common::dollars(1), at_ceiling);
    assert_eq!(
        env.reserve(&usdc).config.max_borrow_rate_bps,
        BORROW_RATE_CEILING_BPS
    );

    let flat_at_ceiling = config_with_curve(
        BORROW_RATE_CEILING_BPS,
        BORROW_RATE_CEILING_BPS,
        BORROW_RATE_CEILING_BPS,
    );
    env.try_update_config(&usdc, flat_at_ceiling).unwrap();
    let config = env.reserve(&usdc).config;
    assert_eq!(config.min_borrow_rate_bps, BORROW_RATE_CEILING_BPS);
    assert_eq!(config.optimal_borrow_rate_bps, BORROW_RATE_CEILING_BPS);
    assert_eq!(config.max_borrow_rate_bps, BORROW_RATE_CEILING_BPS);
}

/// Lowering the liquidation threshold of a reserve backing an open borrow is
/// refused: it would move the line the borrower is measured against.
#[test]
fn rejects_lowering_liquidation_threshold() {
    let mut env = Env::new();
    let (collateral, _borrow) = half_borrowed_reserve(&mut env);
    let mut lower = default_config();
    lower.liquidation_threshold_bps = 7_900;
    let result = env.try_update_config(&collateral, lower);
    common::assert_program_error!(result, LendingError::RiskLimitLowered);
    assert_eq!(
        env.reserve(&collateral).config.liquidation_threshold_bps,
        8_000
    );
}

/// Lowering the loan-to-value is accepted, down to 0: it limits only new
/// borrows, so the open borrow stays healthy and cannot be liquidated, while a
/// new borrow past the lower limit is refused.
#[test]
fn accepts_lowering_loan_to_value() {
    let mut env = Env::new();
    let collateral = env.add_reserve(6, common::dollars(1), default_config());
    let borrow = env.add_reserve(6, common::dollars(1), default_config());
    let supplier = env.create_user();
    env.fund(&supplier, borrow.mint, 1_000_000_000);
    env.supply(&supplier, &borrow, 1_000_000_000);
    let borrower = env.create_user();
    env.fund(&borrower, collateral.mint, 1_000_000_000);
    env.fund(&borrower, borrow.mint, 0);
    env.supply(&borrower, &collateral, 1_000_000_000);
    let obligation = env.initialize_obligation(&borrower);
    env.post_collateral(&borrower, obligation, &collateral, 1_000_000_000);
    // $700 against $1,000 of collateral: inside the 75% loan-to-value.
    env.try_borrow(
        &borrower,
        obligation,
        &[&collateral],
        &[],
        &borrow,
        700_000_000,
    )
    .unwrap();

    let mut frozen = default_config();
    frozen.loan_to_value_bps = 0;
    env.try_update_config(&collateral, frozen).unwrap();
    assert_eq!(env.reserve(&collateral).config.loan_to_value_bps, 0);

    let result = env.try_borrow(
        &borrower,
        obligation,
        &[&collateral],
        &[&borrow],
        &borrow,
        1,
    );
    common::assert_program_error!(result, LendingError::BorrowTooLarge);

    let liquidator = env.create_user();
    env.fund(&liquidator, borrow.mint, 1_000_000_000);
    let result = env.try_liquidate(
        &liquidator,
        obligation,
        &[&collateral],
        &[&borrow],
        &borrow,
        &collateral,
        100_000_000,
    );
    common::assert_program_error!(result, LendingError::ObligationHealthy);
    assert_eq!(
        env.obligation(obligation).deposits[0].deposited_shares,
        1_000_000_000
    );
}

/// Raising both limits loosens the reserve for every borrower and is accepted.
#[test]
fn accepts_raising_loan_to_value_and_liquidation_threshold() {
    let mut env = Env::new();
    let (collateral, _borrow) = half_borrowed_reserve(&mut env);
    let mut higher = default_config();
    higher.loan_to_value_bps = 7_600;
    higher.liquidation_threshold_bps = 8_100;
    env.try_update_config(&collateral, higher).unwrap();
    let config = env.reserve(&collateral).config;
    assert_eq!(config.loan_to_value_bps, 7_600);
    assert_eq!(config.liquidation_threshold_bps, 8_100);
}

/// An update that leaves both limits where they are and moves the rate curve
/// within the ceiling is accepted.
#[test]
fn accepts_curve_change_with_risk_limits_unchanged() {
    let mut env = Env::new();
    let (_collateral, borrow) = half_borrowed_reserve(&mut env);
    let steeper = config_with_curve(500, 5_000, BORROW_RATE_CEILING_BPS);
    env.try_update_config(&borrow, steeper).unwrap();
    let config = env.reserve(&borrow).config;
    assert_eq!(config.loan_to_value_bps, 7_500);
    assert_eq!(config.liquidation_threshold_bps, 8_000);
    assert_eq!(config.min_borrow_rate_bps, 500);
    assert_eq!(config.optimal_borrow_rate_bps, 5_000);
    assert_eq!(config.max_borrow_rate_bps, BORROW_RATE_CEILING_BPS);
}

/// A config whose liquidation threshold and bonus are `threshold` and
/// `bonus`, every other field the default.
fn config_with_threshold_and_bonus(threshold: u16, bonus: u16) -> lending::state::ReserveConfig {
    let mut config = default_config();
    config.liquidation_threshold_bps = threshold;
    config.liquidation_bonus_bps = bonus;
    config
}

/// A reserve cannot be created with a threshold so high that a liquidation at
/// it could not pay the bonus from the collateral: 8,000 x 12,501 is past
/// 10,000 x 10,000.
#[test]
fn rejects_unpayable_liquidation_bonus_at_initialize() {
    let mut env = Env::new();
    let owner = env.owner.insecure_clone();
    let market = env.market;
    let unpayable = config_with_threshold_and_bonus(8_000, 2_501);
    let result = env.try_add_reserve_to(&owner, market, 6, common::dollars(1), unpayable);
    common::assert_program_error!(result.map(|_| ()), LendingError::LiquidationBonusUnpayable);
}

/// An update may not cross the bound either, by raising the bonus or by
/// raising the threshold, and the config is left as it was.
#[test]
fn rejects_unpayable_liquidation_bonus_on_update() {
    let mut env = Env::new();
    let (collateral, _borrow) = half_borrowed_reserve(&mut env);
    // 8,000 x 12,501 and 9,524 x 10,500 are each past 100,000,000.
    for (threshold, bonus) in [(8_000, 2_501), (9_524, 500)] {
        let result = env.try_update_config(
            &collateral,
            config_with_threshold_and_bonus(threshold, bonus),
        );
        common::assert_program_error!(result, LendingError::LiquidationBonusUnpayable);
    }
    let config = env.reserve(&collateral).config;
    assert_eq!(config.liquidation_threshold_bps, 8_000);
    assert_eq!(config.liquidation_bonus_bps, 500);
}

/// A threshold and bonus exactly at the bound are accepted, at creation and on
/// update: 8,000 x 12,500 is exactly 100,000,000, and 9,523 is the highest
/// threshold a 5% bonus allows (9,523 x 10,500 is 99,991,500).
#[test]
fn accepts_liquidation_bonus_at_the_bound() {
    let mut env = Env::new();
    let usdc = env.add_reserve(
        6,
        common::dollars(1),
        config_with_threshold_and_bonus(8_000, 2_500),
    );
    assert_eq!(env.reserve(&usdc).config.liquidation_bonus_bps, 2_500);

    let (collateral, _borrow) = half_borrowed_reserve(&mut env);
    env.try_update_config(&collateral, config_with_threshold_and_bonus(8_000, 2_500))
        .unwrap();
    assert_eq!(env.reserve(&collateral).config.liquidation_bonus_bps, 2_500);
    env.try_update_config(&collateral, config_with_threshold_and_bonus(9_523, 500))
        .unwrap();
    let config = env.reserve(&collateral).config;
    assert_eq!(config.liquidation_threshold_bps, 9_523);
    assert_eq!(config.liquidation_bonus_bps, 500);
}
