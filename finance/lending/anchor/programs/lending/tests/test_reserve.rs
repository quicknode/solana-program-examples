mod common;

use common::{default_config, Env};
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
    assert!(
        result.unwrap_err().contains("InvalidConfig"),
        "LTV above the liquidation threshold must be rejected"
    );
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
    assert!(result.unwrap_err().contains("InvalidConfig"));
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
