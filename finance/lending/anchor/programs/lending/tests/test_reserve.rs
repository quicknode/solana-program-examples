mod common;

use lending::errors::LendingError;

use common::{default_config, Env};
use lending::constants::FIXED_POINT_SCALE;
use lending::state::Reserve;

#[test]
fn init_market_and_reserve() {
    let mut env = Env::new();
    let usdc = env.add_reserve(6, common::dollars(1), default_config());

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

#[test]
fn accepts_valid_config_update() {
    let mut env = Env::new();
    let usdc = env.add_reserve(6, common::dollars(1), default_config());

    let mut updated = default_config();
    updated.loan_to_value_bps = 6_000;
    env.try_update_config(&usdc, updated).unwrap();
    assert_eq!(env.reserve(&usdc).config.loan_to_value_bps, 6_000);
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
