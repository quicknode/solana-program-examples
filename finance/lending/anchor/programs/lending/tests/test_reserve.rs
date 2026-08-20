mod common;

use lending::errors::LendingError;

use common::{default_config, Env, SLOTS_PER_YEAR};
use lending::constants::FIXED_POINT_SCALE;

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

#[test]
fn rejects_zero_slots_per_year() {
    let mut env = Env::new();
    let usdc = env.add_reserve(6, common::dollars(1), default_config());

    let mut bad = default_config();
    bad.slots_per_year = 0;
    let result = env.try_update_config(&usdc, bad);
    common::assert_program_error!(result, LendingError::InvalidConfig);
}

/// The rate fields are annual; what a borrower is charged per slot is the APR
/// divided by `slots_per_year`. Two reserves differing only in that divisor
/// accrue in proportion to it over the same elapsed slots. This is why the
/// cluster's slot time has to be configured rather than compiled in: leave a
/// stale figure in place after the protocol shortens the slot and every
/// borrower pays more per day than the advertised APR, with nothing in the
/// program changed to say so.
#[test]
fn slots_per_year_scales_the_per_slot_rate() {
    let mut env = Env::new();
    let collateral = env.add_reserve(6, common::dollars(1), default_config());

    let baseline = env.add_reserve(6, common::dollars(1), default_config());
    let mut halved_config = default_config();
    halved_config.slots_per_year = SLOTS_PER_YEAR / 2;
    let halved = env.add_reserve(6, common::dollars(1), halved_config);

    // Identical supply and borrow in both, so both sit at 50% utilization and
    // therefore resolve to the same APR from the same kinked curve.
    for reserve in [&baseline, &halved] {
        let supplier = env.create_user();
        env.fund(&supplier, reserve.mint, 1_000_000_000);
        env.supply(&supplier, reserve, 1_000_000_000);

        let borrower = env.create_user();
        env.fund(&borrower, collateral.mint, 1_000_000_000);
        env.fund(&borrower, reserve.mint, 0);
        env.supply(&borrower, &collateral, 1_000_000_000);
        let obligation = env.initialize_obligation(&borrower);
        env.post_collateral(&borrower, obligation, &collateral, 1_000_000_000);
        env.try_borrow(
            &borrower,
            obligation,
            &[&collateral],
            &[],
            reserve,
            500_000_000,
        )
        .unwrap();
    }

    let elapsed = SLOTS_PER_YEAR / 100;
    env.warp_slots(elapsed);
    env.set_price(collateral.mint, common::dollars(1));
    env.set_price(baseline.mint, common::dollars(1));
    env.set_price(halved.mint, common::dollars(1));

    let refresher = env.create_user();
    env.refresh_reserve_only(&refresher, &baseline);
    env.refresh_reserve_only(&refresher, &halved);

    let baseline_growth = env.reserve(&baseline).borrow_accumulation_factor - FIXED_POINT_SCALE;
    let halved_growth = env.reserve(&halved).borrow_accumulation_factor - FIXED_POINT_SCALE;
    assert!(
        baseline_growth > 0,
        "the baseline reserve must have accrued something to compare against"
    );

    // The per-slot rate is floored, so the two rates can differ by one unit in
    // the last place; over `elapsed` slots that is an `elapsed`-sized gap.
    let doubled = baseline_growth * 2;
    assert!(
        halved_growth.abs_diff(doubled) <= elapsed as u128,
        "halving slots_per_year should double the accrual: got {halved_growth}, expected about {doubled}"
    );
}
