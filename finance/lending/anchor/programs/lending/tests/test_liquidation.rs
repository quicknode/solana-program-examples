mod common;

use common::{ata, cents, default_config, dollars, Env, ReserveHandle};
use solana_keypair::Keypair;
use solana_signer::Signer;

/// A borrower with $1000 of collateral who has borrowed $700 (healthy at 80%
/// liquidation threshold), plus a liquidator funded with the borrow token.
fn setup() -> (
    Env,
    ReserveHandle,
    ReserveHandle,
    Keypair,
    anchor_lang::prelude::Pubkey,
    Keypair,
) {
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
    env.try_borrow(&borrower, obligation, &[&collateral], &[], &borrow, 700_000_000)
        .unwrap();

    let liquidator = env.create_user();
    env.fund(&liquidator, borrow.mint, 1_000_000_000);

    (env, collateral, borrow, borrower, obligation, liquidator)
}

#[test]
fn healthy_obligation_cannot_be_liquidated() {
    let (mut env, collateral, borrow, _borrower, obligation, liquidator) = setup();
    let result = env.try_liquidate(
        &liquidator,
        obligation,
        &[&collateral],
        &[&borrow],
        &borrow,
        &collateral,
        100_000_000,
    );
    assert!(result.unwrap_err().contains("ObligationHealthy"));
}

#[test]
fn unhealthy_obligation_liquidated_with_bonus_capped_by_close_factor() {
    let (mut env, collateral, borrow, _borrower, obligation, liquidator) = setup();

    // Collateral price falls to $0.80: collateral value $800, liquidation
    // threshold 80% => $640, while debt is $700 => liquidatable.
    env.set_price(collateral.mint, cents(80));

    let liquidator_repay_account = ata(&liquidator.pubkey(), &borrow.mint);
    let liquidator_collateral_account = ata(&liquidator.pubkey(), &collateral.share_mint);
    let vault_before = env.reserve(&borrow).available_liquidity;

    // Offer to repay far more than the close factor allows; it caps at 50% of the
    // $700 debt = $350.
    env.try_liquidate(
        &liquidator,
        obligation,
        &[&collateral],
        &[&borrow],
        &borrow,
        &collateral,
        1_000_000_000,
    )
    .unwrap();

    // Exactly $350 (350M base units) was repaid — close-factor cap, not the full offer.
    assert_eq!(
        env.token_balance(liquidator_repay_account),
        1_000_000_000 - 350_000_000
    );
    assert_eq!(
        env.reserve(&borrow).available_liquidity,
        vault_before + 350_000_000
    );

    // Liquidator seized collateral shares worth repay + 5% bonus, priced at $0.80:
    // (350 * 1.05) / 0.80 = 459.375 collateral units => 459_375_000 shares (1:1 here).
    assert_eq!(
        env.token_balance(liquidator_collateral_account),
        459_375_000
    );

    // The borrower's debt and collateral both dropped.
    let obligation_state = env.obligation(obligation);
    assert_eq!(obligation_state.deposits[0].deposited_shares, 1_000_000_000 - 459_375_000);
}

/// A repayment whose seizure would exceed the posted collateral is rejected
/// rather than silently capped — silently capping would make the liquidator
/// pay full price for less collateral. A smaller repayment still works.
#[test]
fn over_seizing_liquidation_rejected_smaller_succeeds() {
    let (mut env, collateral, borrow, _borrower, obligation, liquidator) = setup();

    // Collateral crashes to $0.10: $100 of collateral against $700 of debt.
    // The close-factor max repay ($350, plus 5% bonus => $367.50 of collateral)
    // would seize far more than the $100 posted.
    env.set_price(collateral.mint, cents(10));

    let over_seize = env.try_liquidate(
        &liquidator,
        obligation,
        &[&collateral],
        &[&borrow],
        &borrow,
        &collateral,
        350_000_000,
    );
    assert!(over_seize.unwrap_err().contains("LiquidationTooLarge"));

    // Repaying $50 seizes $52.50 of collateral = 525 units at $0.10 — fits.
    env.try_liquidate(
        &liquidator,
        obligation,
        &[&collateral],
        &[&borrow],
        &borrow,
        &collateral,
        50_000_000,
    )
    .unwrap();
    let liquidator_collateral_account = ata(&liquidator.pubkey(), &collateral.share_mint);
    assert_eq!(env.token_balance(liquidator_collateral_account), 525_000_000);
}
