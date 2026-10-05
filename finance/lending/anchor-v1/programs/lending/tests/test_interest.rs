mod common;

use common::{ata, default_config, dollars, Env, TENTH_OF_A_YEAR};
use lending::constants::FIXED_POINT_SCALE;
use solana_signer::Signer;

/// Borrowing at non-zero utilization, then letting time pass, must grow the
/// reserve's accumulation factor, the borrower's debt, and the share exchange rate.
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
    env.try_borrow(
        &borrower,
        obligation,
        &[&collateral],
        &[],
        &borrow,
        500_000_000,
    )
    .unwrap();

    assert_eq!(
        env.reserve(&borrow).borrow_accumulation_factor,
        FIXED_POINT_SCALE
    );

    // Let a tenth of a year pass, then re-publish prices and refresh.
    env.warp_seconds(TENTH_OF_A_YEAR);
    env.set_price(collateral.mint, dollars(1));
    env.set_price(borrow.mint, dollars(1));
    env.refresh_reserve_only(&borrower, &borrow);

    let index_after = env.reserve(&borrow).borrow_accumulation_factor;
    assert!(
        index_after > FIXED_POINT_SCALE,
        "accumulation factor must grow once time passes with outstanding borrows"
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

/// The program keeps `reserve_factor_bps` of accrued interest as fees the
/// market owner can withdraw, while the rest lifts the supplier exchange rate.
#[test]
fn program_fees_accrue_and_owner_can_collect() {
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
    env.try_borrow(
        &borrower,
        obligation,
        &[&collateral],
        &[],
        &borrow,
        500_000_000,
    )
    .unwrap();

    // No interest has accrued yet, so no fees.
    assert_eq!(env.reserve(&borrow).accumulated_program_fees, 0);

    env.warp_seconds(TENTH_OF_A_YEAR);
    env.refresh_reserve_only(&borrower, &borrow);

    // Fees accrued: 10% (the reserve factor) of the interest, rounded up.
    let reserve = env.reserve(&borrow);
    let fees = reserve.accumulated_program_fees;
    assert!(fees > 0, "program fees should accrue once interest does");
    let total_interest = reserve.current_borrowed_amount().unwrap() - 500_000_000;
    let expected_fee = total_interest.div_ceil(10); // 1000 bps = 10%
    assert_eq!(
        fees, expected_fee,
        "fees {fees} should be 10% of interest {total_interest}, rounded up"
    );

    // Maria withdraws the fees to her own account.
    let owner_account = env.collect_program_fees(&borrow);
    assert_eq!(env.token_balance(owner_account), fees);
    assert_eq!(env.reserve(&borrow).accumulated_program_fees, 0);
}

/// The program fee is the program's cut of the interest, so it rounds against
/// the user: up. One second of interest on a 500-unit borrow at the default
/// curve is 3 units, a tenth of which is not whole; the fee is 1, not 0, and
/// the suppliers' pool grows by the other 2, so fee and remainder sum to the
/// interest and never exceed it.
#[test]
fn program_fee_rounds_up_and_suppliers_take_the_remainder() {
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
    env.try_borrow(
        &borrower,
        obligation,
        &[&collateral],
        &[],
        &borrow,
        500_000_000,
    )
    .unwrap();
    let before = env.reserve(&borrow);
    let debt_before = before.current_borrowed_amount().unwrap();
    let suppliers_before = before.total_liquidity().unwrap();

    env.warp_seconds(1);
    env.refresh_reserve_only(&borrower, &borrow);

    let after = env.reserve(&borrow);
    let interest = after.current_borrowed_amount().unwrap() - debt_before;
    assert_eq!(interest, 3, "one second of interest on the 500-unit borrow");
    let fee = after.accumulated_program_fees;
    assert_eq!(fee, 1, "a tenth of 3 units rounds up to 1 for the program");
    assert_eq!(
        after.total_liquidity().unwrap(),
        suppliers_before + (interest - fee) as u128,
        "the suppliers' pool grows by the interest less the fee"
    );
}
