mod common;

use anchor_v2_testing::Signer;
use lending::errors::LendingError;

use common::{ata, default_config, dollars, Env};

/// After interest makes the pool worth more than its share supply, a deposit so
/// small it would mint zero shares is rejected rather than silently giving the
/// depositor nothing.
#[test]
fn deposit_that_would_mint_zero_shares_is_rejected() {
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

    // Accrue enough interest that total liquidity exceeds the share supply.
    env.warp_seconds(common::TENTH_OF_A_YEAR);
    env.refresh_reserve_only(&borrower, &borrow);
    assert!(
        env.reserve(&borrow).borrow_accumulation_factor > lending::constants::FIXED_POINT_SCALE
    );

    let dust_depositor = env.create_user();
    env.fund(&dust_depositor, borrow.mint, 1);
    let result = env.try_supply(&dust_depositor, &borrow, 1);
    common::assert_program_error!(result, LendingError::DepositTooSmall);
}

/// Deposits floor the shares minted and redemptions floor the liquidity paid
/// out, so a supplier who deposits and redeems over and over, at a size that
/// does not divide the exchange rate evenly, can never end up with more than
/// they started with. Interest accrues first so the rate is not one-to-one,
/// and the pool stays borrowed throughout so every trip rounds somewhere.
#[test]
fn deposit_redeem_round_trip_creates_no_value() {
    let mut env = Env::new();
    let collateral = env.add_reserve(6, dollars(1), default_config());
    let usdc = env.add_reserve(6, dollars(1), default_config());

    let supplier = env.create_user();
    env.fund(&supplier, usdc.mint, 1_000_000_000);
    env.supply(&supplier, &usdc, 1_000_000_000);

    let borrower = env.create_user();
    env.fund(&borrower, collateral.mint, 1_000_000_000);
    env.fund(&borrower, usdc.mint, 0);
    env.supply(&borrower, &collateral, 1_000_000_000);
    let obligation = env.initialize_obligation(&borrower);
    env.post_collateral(&borrower, obligation, &collateral, 1_000_000_000);
    env.try_borrow(
        &borrower,
        obligation,
        &[&collateral],
        &[],
        &usdc,
        500_000_000,
    )
    .unwrap();
    env.warp_seconds(common::TENTH_OF_A_YEAR);
    env.refresh_reserve_only(&borrower, &usdc);
    assert!(env.reserve(&usdc).borrow_accumulation_factor > lending::constants::FIXED_POINT_SCALE);

    let user = env.create_user();
    let amount = 777_777_777;
    // Funded with more than one deposit's worth, so a trip that loses dust
    // leaves enough for the next deposit of the full amount.
    let funded = 2 * amount;
    let liquidity_account = env.fund(&user, usdc.mint, funded);
    let round_trips = 50;

    for trip in 1..=round_trips {
        let share_account = env.supply(&user, &usdc, amount);
        let shares = env.token_balance(share_account);
        env.try_redeem(&user, &usdc, shares).unwrap();
        // The next trip's instructions are byte-identical to this one's.
        env.svm.expire_blockhash();

        assert!(
            env.token_balance(liquidity_account) <= funded,
            "round trip {trip} returned more than was put in"
        );
    }
}

#[test]
fn withdraw_at_health_boundary_then_one_more_unit_fails() {
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

    // Borrow $600 against $1000 collateral (75% LTV => $750 power).
    env.try_borrow(
        &borrower,
        obligation,
        &[&collateral],
        &[],
        &borrow,
        600_000_000,
    )
    .unwrap();

    // Withdrawing $200 of collateral lands exactly on the limit: new power
    // $750 - 0.75*$200 = $600 == debt. This must pass.
    env.try_withdraw_collateral(
        &borrower,
        obligation,
        &[&collateral],
        &[&borrow],
        &collateral,
        200_000_000,
    )
    .unwrap();
    assert_eq!(
        env.token_balance(ata(&borrower.pubkey(), &collateral.share_mint)),
        200_000_000
    );

    // One more unit now pushes the obligation past its limit.
    let result = env.try_withdraw_collateral(
        &borrower,
        obligation,
        &[&collateral],
        &[&borrow],
        &collateral,
        1,
    );
    common::assert_program_error!(result, LendingError::WithdrawTooLarge);
}
