mod common;

use common::{ata, default_config, dollars, Env, ReserveHandle};
use solana_keypair::Keypair;
use solana_signer::Signer;

/// One market with a collateral reserve and a separately-supplied borrow
/// reserve, plus a borrower who has posted 1000 units of collateral (value
/// $1000, so 75% LTV => $750 borrow power). Both tokens priced at $1, 6 decimals.
fn setup() -> (Env, ReserveHandle, ReserveHandle, Keypair, anchor_lang::prelude::Pubkey) {
    let mut env = Env::new();
    let collateral = env.add_reserve(6, dollars(1), default_config());
    let borrow = env.add_reserve(6, dollars(1), default_config());

    // A different supplier funds the borrow reserve's liquidity.
    let supplier = env.create_user();
    env.fund(&supplier, borrow.mint, 1_000_000_000);
    env.supply(&supplier, &borrow, 1_000_000_000);

    let borrower = env.create_user();
    env.fund(&borrower, collateral.mint, 1_000_000_000);
    env.fund(&borrower, borrow.mint, 0); // create the borrowed-token account
    env.supply(&borrower, &collateral, 1_000_000_000);
    let obligation = env.initialize_obligation(&borrower);
    env.post_collateral(&borrower, obligation, &collateral, 1_000_000_000);

    (env, collateral, borrow, borrower, obligation)
}

#[test]
fn borrow_up_to_max_ltv_then_one_more_fails() {
    let (mut env, collateral, borrow, borrower, obligation) = setup();

    // $750 of borrow power, borrowing a $1 token => 750 units exactly.
    env.try_borrow(&borrower, obligation, &[&collateral], &[], &borrow, 750_000_000)
        .unwrap();
    assert_eq!(
        env.token_balance(ata(&borrower.pubkey(), &borrow.mint)),
        750_000_000
    );

    // One more unit exceeds the allowed borrow value.
    let result = env.try_borrow(&borrower, obligation, &[&collateral], &[&borrow], &borrow, 1);
    assert!(
        result.unwrap_err().contains("BorrowTooLarge"),
        "borrowing past the LTV limit must be rejected"
    );
}

#[test]
fn borrow_without_obligation_refresh_is_rejected() {
    let (mut env, collateral, borrow, borrower, obligation) = setup();
    let result = env.try_borrow_skip_obligation_refresh(
        &borrower,
        obligation,
        &[&collateral, &borrow],
        &borrow,
        100_000_000,
    );
    assert!(result.unwrap_err().contains("ObligationStale"));
}

#[test]
fn borrow_with_stale_price_feed_is_rejected() {
    let (mut env, collateral, borrow, borrower, obligation) = setup();
    // Advance well past the staleness window without re-publishing prices.
    env.warp_slots(50);
    let result = env.try_borrow(&borrower, obligation, &[&collateral], &[], &borrow, 100_000_000);
    assert!(result.unwrap_err().contains("StalePriceFeed"));
}

/// A cluster restart passes hours of wall-clock time in zero slots, so a price
/// published before the halt can still look fresh by slot count. The feed must
/// reject it until the publisher posts again.
#[test]
fn borrow_with_price_from_before_a_restart_is_rejected() {
    let (mut env, collateral, borrow, borrower, obligation) = setup();

    // The prices were published at the current slot. Simulate a halt: the
    // cluster restarts a few slots later, well inside the staleness window,
    // so only the restart check can catch the pre-halt price.
    let restart_slot = env.current_slot() + 3;
    env.warp_slots(5);
    env.set_last_restart_slot(restart_slot);

    let result = env.try_borrow(&borrower, obligation, &[&collateral], &[], &borrow, 100_000_000);
    assert!(
        result.unwrap_err().contains("PricePredatesRestart"),
        "a pre-restart price must be rejected even inside the staleness window"
    );

    // Publishing after the restart reopens the market. Warp first: the retry is
    // otherwise byte-identical to the rejected borrow, so it would carry the
    // same signature and be dropped as already processed. The failed borrow
    // recorded nothing, so the obligation still has no borrows to refresh.
    env.warp_slots(1);
    env.set_price(collateral.mint, dollars(1));
    env.set_price(borrow.mint, dollars(1));
    env.try_borrow(&borrower, obligation, &[&collateral], &[], &borrow, 100_000_000)
        .expect("a freshly published price must be accepted after a restart");
}

#[test]
fn repay_reduces_debt_and_over_repay_clamps() {
    let (mut env, collateral, borrow, borrower, obligation) = setup();
    env.try_borrow(&borrower, obligation, &[&collateral], &[], &borrow, 500_000_000)
        .unwrap();
    assert_eq!(env.reserve(&borrow).borrowed_principal > 0, true);

    env.repay(&borrower, obligation, &borrow, 200_000_000);
    let obligation_state = env.obligation(obligation);
    assert_eq!(obligation_state.borrows.len(), 1);

    // Over-repay: ask to repay far more than owed; it clamps to the remaining debt.
    env.repay(&borrower, obligation, &borrow, 1_000_000_000);
    assert_eq!(env.reserve(&borrow).borrowed_principal, 0);
    assert!(env.obligation(obligation).borrows.is_empty());
}

#[test]
fn withdraw_blocked_while_borrowed_then_allowed_after_repay() {
    let (mut env, collateral, borrow, borrower, obligation) = setup();
    env.try_borrow(&borrower, obligation, &[&collateral], &[], &borrow, 750_000_000)
        .unwrap();

    // At the LTV limit, withdrawing any collateral would undercollateralize.
    let blocked = env.try_withdraw_collateral(
        &borrower,
        obligation,
        &[&collateral],
        &[&borrow],
        &collateral,
        100_000_000,
    );
    assert!(blocked.unwrap_err().contains("WithdrawTooLarge"));

    // Repay everything, then the collateral is free to withdraw.
    env.repay(&borrower, obligation, &borrow, 750_000_000);
    env.try_withdraw_collateral(
        &borrower,
        obligation,
        &[&collateral],
        &[],
        &collateral,
        1_000_000_000,
    )
    .unwrap();
    assert_eq!(
        env.token_balance(ata(&borrower.pubkey(), &collateral.share_mint)),
        1_000_000_000
    );
}
