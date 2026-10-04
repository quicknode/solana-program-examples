mod common;

use anchor_v2_testing::{Keypair, Signer};
use lending::errors::LendingError;

use common::{
    ata, cents, default_config, dollars, narrow_band, widest_accepted_band, Env, ReserveHandle,
    DEFAULT_MAX_CONFIDENCE_BPS,
};

/// One market with a collateral reserve and a separately-supplied borrow
/// reserve, plus a borrower who has posted 1000 units of collateral (value
/// $1000, so 75% LTV => $750 borrow power). Both tokens priced at $1, 6 decimals.
fn setup() -> (
    Env,
    ReserveHandle,
    ReserveHandle,
    Keypair,
    solana_pubkey::Pubkey,
) {
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
    env.try_borrow(
        &borrower,
        obligation,
        &[&collateral],
        &[],
        &borrow,
        750_000_000,
    )
    .unwrap();
    assert_eq!(
        env.token_balance(ata(&borrower.pubkey(), &borrow.mint)),
        750_000_000
    );

    // One more unit exceeds the allowed borrow value.
    let result = env.try_borrow(
        &borrower,
        obligation,
        &[&collateral],
        &[&borrow],
        &borrow,
        1,
    );
    common::assert_program_error!(result, LendingError::BorrowTooLarge);
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
    common::assert_program_error!(result, LendingError::ObligationStale);
}

#[test]
fn borrow_with_stale_price_feed_is_rejected() {
    let (mut env, collateral, borrow, borrower, obligation) = setup();
    // Advance well past the staleness window without re-publishing prices.
    env.warp_slots(50);
    let result = env.try_borrow(
        &borrower,
        obligation,
        &[&collateral],
        &[],
        &borrow,
        100_000_000,
    );
    common::assert_program_error!(result, LendingError::StalePriceFeed);
}

/// A price the oracle is unsure of is no price to lend against. The
/// collateral feed is read by `refresh_obligation`, so the band is refused
/// there, before the borrow handler runs; once the publisher posts a narrow
/// band again the same borrow goes through.
#[test]
fn borrow_against_collateral_priced_with_a_wide_band_is_rejected() {
    let (mut env, collateral, borrow, borrower, obligation) = setup();

    // Twice the band the reserve allows.
    let wide_band = 2 * widest_accepted_band(dollars(1), DEFAULT_MAX_CONFIDENCE_BPS);
    env.set_price_with_confidence(collateral.mint, dollars(1), wide_band);
    let result = env.try_borrow(
        &borrower,
        obligation,
        &[&collateral],
        &[],
        &borrow,
        100_000_000,
    );
    // The collateral cannot be valued against a price the oracle is unsure of.
    common::assert_program_error!(result, LendingError::OracleConfidenceTooWide);

    // Warp so the retry is not byte-identical to the rejected borrow.
    env.warp_slots(1);
    env.set_price(collateral.mint, dollars(1));
    env.set_price(borrow.mint, dollars(1));
    env.try_borrow(
        &borrower,
        obligation,
        &[&collateral],
        &[],
        &borrow,
        100_000_000,
    )
    .unwrap();
    assert_eq!(
        env.token_balance(ata(&borrower.pubkey(), &borrow.mint)),
        100_000_000
    );
}

/// The borrowed token's feed is read by the borrow handler itself, which
/// applies the same limit, so a wide band on that side is refused too.
#[test]
fn borrow_of_a_token_priced_with_a_wide_band_is_rejected() {
    let (mut env, collateral, borrow, borrower, obligation) = setup();

    let wide_band = 2 * widest_accepted_band(dollars(1), DEFAULT_MAX_CONFIDENCE_BPS);
    env.set_price_with_confidence(borrow.mint, dollars(1), wide_band);
    let result = env.try_borrow(
        &borrower,
        obligation,
        &[&collateral],
        &[],
        &borrow,
        100_000_000,
    );
    common::assert_program_error!(result, LendingError::OracleConfidenceTooWide);
}

/// The limit is inclusive: a band of exactly `max_confidence_bps` of the
/// price is accepted, and one unit wider is refused. At $1.23 the 1% limit is
/// 12,300,000,000,000,000 in the mantissa's units, with no rounding to hide
/// behind.
#[test]
fn confidence_band_at_the_limit_passes_and_one_unit_over_fails() {
    let (mut env, collateral, borrow, borrower, obligation) = setup();
    let price = cents(123);
    let limit = widest_accepted_band(price, DEFAULT_MAX_CONFIDENCE_BPS);
    assert_eq!(limit, 12_300_000_000_000_000);

    env.set_price_with_confidence(collateral.mint, price, limit + 1);
    let result = env.try_borrow(
        &borrower,
        obligation,
        &[&collateral],
        &[],
        &borrow,
        100_000_000,
    );
    // One unit past the limit must be refused.
    common::assert_program_error!(result, LendingError::OracleConfidenceTooWide);

    env.warp_slots(1);
    env.set_price_with_confidence(collateral.mint, price, limit);
    env.set_price_with_confidence(borrow.mint, dollars(1), narrow_band(dollars(1)));
    env.try_borrow(
        &borrower,
        obligation,
        &[&collateral],
        &[],
        &borrow,
        100_000_000,
    )
    .unwrap();
    assert_eq!(
        env.token_balance(ata(&borrower.pubkey(), &borrow.mint)),
        100_000_000
    );
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

    let result = env.try_borrow(
        &borrower,
        obligation,
        &[&collateral],
        &[],
        &borrow,
        100_000_000,
    );
    common::assert_program_error!(result, LendingError::PricePredatesRestart);

    // Publishing after the restart reopens the market. Warp first: the retry is
    // otherwise byte-identical to the rejected borrow, so it would carry the
    // same signature and be dropped as already processed. The failed borrow
    // recorded nothing, so the obligation still has no borrows to refresh.
    env.warp_slots(1);
    env.set_price(collateral.mint, dollars(1));
    env.set_price(borrow.mint, dollars(1));
    env.try_borrow(
        &borrower,
        obligation,
        &[&collateral],
        &[],
        &borrow,
        100_000_000,
    )
    .expect("a freshly published price must be accepted after a restart");
}

#[test]
fn repay_reduces_debt_and_over_repay_clamps() {
    let (mut env, collateral, borrow, borrower, obligation) = setup();
    env.try_borrow(
        &borrower,
        obligation,
        &[&collateral],
        &[],
        &borrow,
        500_000_000,
    )
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
    env.try_borrow(
        &borrower,
        obligation,
        &[&collateral],
        &[],
        &borrow,
        750_000_000,
    )
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
    common::assert_program_error!(blocked, LendingError::WithdrawTooLarge);

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
