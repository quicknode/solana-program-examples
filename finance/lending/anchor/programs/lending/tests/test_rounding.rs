mod common;

use common::{ata, default_config, dollars, Env};
use solana_signer::Signer;

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
    let obligation = env.init_obligation(&borrower);
    env.post_collateral(&borrower, obligation, &collateral, 1_000_000_000);
    env.try_borrow(&borrower, obligation, &[&collateral], &[], &borrow, 500_000_000)
        .unwrap();

    // Accrue enough interest that total liquidity exceeds the share supply.
    env.warp_slots(7_884_000);
    env.refresh_reserve_only(&borrower, &borrow);
    assert!(env.reserve(&borrow).cumulative_borrow_rate_index > lending::constants::FIXED_POINT_SCALE);

    let dust_depositor = env.create_user();
    env.fund(&dust_depositor, borrow.mint, 1);
    let result = env.try_supply(&dust_depositor, &borrow, 1);
    assert!(
        result.unwrap_err().contains("DepositTooSmall"),
        "a 1-unit deposit into an appreciated pool mints zero shares and must be rejected"
    );
}

#[test]
fn deposit_redeem_round_trip_creates_no_value() {
    let mut env = Env::new();
    let usdc = env.add_reserve(6, dollars(1), default_config());

    let user = env.create_user();
    let amount = 777_777_777;
    let liquidity_account = env.fund(&user, usdc.mint, amount);
    let share_account = env.supply(&user, &usdc, amount);

    let shares = env.token_balance(share_account);
    env.try_redeem(&user, &usdc, shares).unwrap();

    // The round trip must never return more than was put in.
    assert!(env.token_balance(liquidity_account) <= amount);
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
    let obligation = env.init_obligation(&borrower);
    env.post_collateral(&borrower, obligation, &collateral, 1_000_000_000);

    // Borrow $600 against $1000 collateral (75% LTV => $750 power).
    env.try_borrow(&borrower, obligation, &[&collateral], &[], &borrow, 600_000_000)
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
    assert!(result.unwrap_err().contains("WithdrawTooLarge"));
}
