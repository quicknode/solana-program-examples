mod common;

use common::{
    ata, cents, default_config, dollars, narrow_band, widest_accepted_band, Env, ReserveHandle,
    DEFAULT_MAX_CONFIDENCE_BPS,
};
use solana_keypair::Keypair;
use solana_signer::Signer;

/// One market with a collateral reserve and a separately-supplied borrow
/// reserve, plus a borrower who has posted 1000 units of collateral (value
/// $1000, so 75% LTV => $750 borrow power). Both tokens priced at $1, 6 decimals.
fn setup() -> (
    Env,
    ReserveHandle,
    ReserveHandle,
    Keypair,
    anchor_lang::prelude::Pubkey,
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
    let result = env.try_borrow(
        &borrower,
        obligation,
        &[&collateral],
        &[],
        &borrow,
        100_000_000,
    );
    assert!(result.unwrap_err().contains("StalePriceFeed"));
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
    assert!(
        result.unwrap_err().contains("OracleConfidenceTooWide"),
        "collateral cannot be valued against a price the oracle is unsure of"
    );

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
    assert!(result.unwrap_err().contains("OracleConfidenceTooWide"));
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
    assert!(
        result.unwrap_err().contains("OracleConfidenceTooWide"),
        "one unit past the limit must be refused"
    );

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

/// A borrower who owes nothing must never be locked in by the oracle. With no
/// borrows the collateral backs nothing, so the withdraw handler reads no
/// price and needs no refresh, and the whole deposit comes out while the feed
/// is stale. The refreshed path is tried first to show the feed really is
/// stale: `refresh_obligation` reads the price and refuses it.
#[test]
fn debt_free_withdraw_needs_no_price_and_no_refresh() {
    let (mut env, collateral, _borrow, borrower, obligation) = setup();
    let user_share = ata(&borrower.pubkey(), &collateral.share_mint);
    let vault = env.obligation_share_vault(&collateral, obligation);
    assert_eq!(env.token_balance(user_share), 0);
    assert_eq!(env.token_balance(vault), 1_000_000_000);

    // Advance well past the staleness window without re-publishing prices.
    env.warp_slots(50);
    let refreshed = env.try_withdraw_collateral(
        &borrower,
        obligation,
        &[&collateral],
        &[],
        &collateral,
        1_000_000_000,
    );
    assert!(refreshed.unwrap_err().contains("StalePriceFeed"));

    env.try_withdraw_collateral_without_refresh(&borrower, obligation, &collateral, 1_000_000_000)
        .unwrap();
    assert_eq!(env.token_balance(user_share), 1_000_000_000);
    // The last share out closes the vault.
    assert!(!env.account_is_open(vault));
    let state = env.obligation(obligation);
    assert!(state.deposits.is_empty());
    assert!(state.borrows.is_empty());
    assert!(state.stale);
}

/// Repaying the last unit removes the borrow entry, so a borrower who has
/// fully repaid is debt-free and withdraws without a price, like one who
/// never borrowed.
#[test]
fn withdraw_after_full_repay_needs_no_price() {
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
    env.repay(&borrower, obligation, &borrow, 500_000_000);
    assert!(env.obligation(obligation).borrows.is_empty());

    env.warp_slots(50);
    env.try_withdraw_collateral_without_refresh(&borrower, obligation, &collateral, 1_000_000_000)
        .unwrap();
    assert_eq!(
        env.token_balance(ata(&borrower.pubkey(), &collateral.share_mint)),
        1_000_000_000
    );
    assert!(env.obligation(obligation).deposits.is_empty());
}

/// With debt outstanding every check stays: a stale price refuses the
/// refreshed withdrawal, and skipping the refresh is refused as stale too.
#[test]
fn withdraw_with_debt_is_refused_while_the_price_is_stale() {
    let (mut env, collateral, borrow, borrower, obligation) = setup();
    env.try_borrow(
        &borrower,
        obligation,
        &[&collateral],
        &[],
        &borrow,
        100_000_000,
    )
    .unwrap();

    env.warp_slots(50);
    let refreshed = env.try_withdraw_collateral(
        &borrower,
        obligation,
        &[&collateral],
        &[&borrow],
        &collateral,
        1,
    );
    assert!(refreshed.unwrap_err().contains("StalePriceFeed"));

    let unrefreshed =
        env.try_withdraw_collateral_without_refresh(&borrower, obligation, &collateral, 1);
    assert!(unrefreshed.unwrap_err().contains("ObligationStale"));

    // Nothing moved.
    assert_eq!(
        env.token_balance(env.obligation_share_vault(&collateral, obligation)),
        1_000_000_000
    );
}
