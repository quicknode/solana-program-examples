mod common;

use common::{default_config, dollars, ata, Env};
use lending::constants::FIXED_POINT_SCALE;
use solana_signer::Signer;

/// Borrowing at non-zero utilization, then letting slots pass, must grow the
/// reserve's interest index, the borrower's debt, and the share exchange rate.
#[test]
fn interest_accrues_on_borrows_over_time() {
    let mut env = Env::new();
    let collateral = env.add_reserve(6, dollars(1), default_config());
    let borrow = env.add_reserve(6, dollars(1), default_config());

    // Supplier funds 1000 units of borrow liquidity.
    let supplier = env.create_user();
    let supplied = 1_000_000_000;
    env.fund(&supplier, borrow.mint, supplied);
    let supplier_liquidity = ata(&supplier.pubkey(), &borrow.mint);
    env.supply(&supplier, &borrow, supplied);

    // Borrower posts collateral and borrows 500 units => 50% utilization.
    let borrower = env.create_user();
    env.fund(&borrower, collateral.mint, 1_000_000_000);
    env.fund(&borrower, borrow.mint, 0);
    env.supply(&borrower, &collateral, 1_000_000_000);
    let obligation = env.initialize_obligation(&borrower);
    env.post_collateral(&borrower, obligation, &collateral, 1_000_000_000);
    env.try_borrow(&borrower, obligation, &[&collateral], &[], &borrow, 500_000_000)
        .unwrap();

    assert_eq!(env.reserve(&borrow).cumulative_borrow_rate_index, FIXED_POINT_SCALE);

    // Let ~0.1 year pass (2.5 slots/s => ~7.884M slots), re-publish prices, refresh.
    env.warp_slots(7_884_000);
    env.set_price(collateral.mint, dollars(1));
    env.set_price(borrow.mint, dollars(1));
    env.refresh_reserve_only(&borrower, &borrow);

    let index_after = env.reserve(&borrow).cumulative_borrow_rate_index;
    assert!(
        index_after > FIXED_POINT_SCALE,
        "interest index must grow once time passes with outstanding borrows"
    );

    // The borrower now owes more than the principal.
    env.refresh_obligation_only(&borrower, obligation, &[&collateral], &[&borrow]);
    let owed_value = env.obligation(obligation).borrowed_value;
    let principal_value = 500u128 * FIXED_POINT_SCALE; // $500 at FIXED_POINT_SCALE per dollar
    assert!(
        owed_value > principal_value,
        "debt value {owed_value} should exceed the $500 principal {principal_value}"
    );

    // The share exchange rate rose: redeeming shares returns more liquidity than
    // was deposited per share. Redeem a slice that fits in available liquidity.
    env.try_redeem(&supplier, &borrow, 100_000_000).unwrap();
    let returned = env.token_balance(supplier_liquidity);
    assert!(
        returned > 100_000_000,
        "100M shares should redeem for more than 100M liquidity after interest, got {returned}"
    );
}

/// The protocol keeps `reserve_factor_bps` of accrued interest as fees the
/// market owner can withdraw, while the rest lifts the supplier exchange rate.
#[test]
fn protocol_fees_accrue_and_owner_can_collect() {
    let mut env = Env::new();
    let collateral = env.add_reserve(6, dollars(1), default_config());
    let borrow = env.add_reserve(6, dollars(1), default_config());

    let supplier = env.create_user();
    env.fund(&supplier, borrow.mint, 1_000_000_000);
    env.supply(&supplier, &borrow, 1_000_000_000);

    let borrower = env.create_user();
    env.fund(&borrower, collateral.mint, 1_000_000_000);
    env.fund(&borrower, borrow.mint, 0);
    env.supply(&borrower, &collateral, 1_000_000_000);
    let obligation = env.initialize_obligation(&borrower);
    env.post_collateral(&borrower, obligation, &collateral, 1_000_000_000);
    env.try_borrow(&borrower, obligation, &[&collateral], &[], &borrow, 500_000_000)
        .unwrap();

    // No interest has accrued yet, so no fees.
    assert_eq!(env.reserve(&borrow).accumulated_protocol_fees, 0);

    env.warp_slots(7_884_000);
    env.refresh_reserve_only(&borrower, &borrow);

    // Fees accrued, and they are ~10% (the reserve factor) of total interest.
    let reserve = env.reserve(&borrow);
    let fees = reserve.accumulated_protocol_fees;
    assert!(fees > 0, "protocol fees should accrue once interest does");
    let total_interest = reserve.current_borrowed_amount().unwrap() - 500_000_000;
    let expected_fee = total_interest / 10; // 1000 bps = 10%
    // Allow a 1-unit rounding tolerance from flooring.
    assert!(
        fees.abs_diff(expected_fee) <= 1,
        "fees {fees} should be ~10% of interest {total_interest}"
    );

    // Maria withdraws the fees to her own account.
    let owner_account = env.collect_protocol_fees(&borrow);
    assert_eq!(env.token_balance(owner_account), fees);
    assert_eq!(env.reserve(&borrow).accumulated_protocol_fees, 0);
}
