mod common;

use anchor_v2_testing::{Keypair, Signer};
use lending::errors::LendingError;

use common::{ata, cents, default_config, dollars, Env, ReserveHandle};

/// A borrower with $1000 of collateral who has borrowed $700 (healthy at 80%
/// liquidation threshold), plus a liquidator funded with the borrow token.
fn setup() -> (
    Env,
    ReserveHandle,
    ReserveHandle,
    Keypair,
    solana_pubkey::Pubkey,
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
    env.try_borrow(
        &borrower,
        obligation,
        &[&collateral],
        &[],
        &borrow,
        700_000_000,
    )
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
    common::assert_program_error!(result, LendingError::ObligationHealthy);
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
    assert_eq!(
        obligation_state.deposits[0].deposited_shares,
        1_000_000_000 - 459_375_000
    );
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
    common::assert_program_error!(over_seize, LendingError::LiquidationTooLarge);

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
    assert_eq!(
        env.token_balance(liquidator_collateral_account),
        525_000_000
    );
}

/// Price at which the capped repayment seizes the whole deposit: at $0.3675
/// the 1,000 collateral units are worth $367.50, and the close factor caps the
/// repayment at half the $700 debt, $350, whose value plus the 5% bonus is
/// exactly $367.50.
const PRICE_TO_SEIZE_EVERYTHING: i128 = 367_500_000_000_000_000;

/// A seizure that takes every collateral share removes the deposit entry and
/// closes the collateral vault. Its rent goes into the obligation account, not
/// to any wallet, so liquidation takes no account the borrower controls; the
/// owner's wallet is untouched until `close_obligation` hands it back.
#[test]
fn seizing_all_collateral_closes_the_vault_into_the_obligation() {
    let (mut env, collateral, borrow, borrower, obligation, liquidator) = setup();
    env.set_price(collateral.mint, PRICE_TO_SEIZE_EVERYTHING);
    let vault = env.obligation_share_vault(&collateral, obligation);
    let vault_rent = env.sol_balance(vault);
    let obligation_before = env.sol_balance(obligation);
    let owner_before = env.sol_balance(borrower.pubkey());

    env.try_liquidate(
        &liquidator,
        obligation,
        &[&collateral],
        &[&borrow],
        &borrow,
        &collateral,
        350_000_000,
    )
    .unwrap();

    assert_eq!(
        env.token_balance(ata(&liquidator.pubkey(), &collateral.share_mint)),
        1_000_000_000
    );
    assert!(env.obligation(obligation).deposits.is_empty());
    assert!(!env.account_is_open(vault));
    assert_eq!(
        env.sol_balance(obligation),
        obligation_before + vault_rent,
        "the vault's rent must land in the obligation"
    );
    assert_eq!(env.sol_balance(borrower.pubkey()), owner_before);
}

/// Share tokens sent straight to the vault are not recorded in the
/// obligation. A seizure that empties the vault sweeps them to the liquidator
/// with the seized shares, so the vault can still close.
#[test]
fn seizing_all_collateral_sweeps_donated_shares_to_the_liquidator() {
    let (mut env, collateral, borrow, _borrower, obligation, liquidator) = setup();
    let vault = env.obligation_share_vault(&collateral, obligation);
    let donor = env.create_user();
    env.fund(&donor, collateral.mint, 5_000_000);
    env.supply(&donor, &collateral, 5_000_000);
    env.send_shares(&donor, &collateral, vault, 5_000_000);
    assert_eq!(env.token_balance(vault), 1_005_000_000);

    env.set_price(collateral.mint, PRICE_TO_SEIZE_EVERYTHING);
    let vault_rent = env.sol_balance(vault);
    let obligation_before = env.sol_balance(obligation);
    env.try_liquidate(
        &liquidator,
        obligation,
        &[&collateral],
        &[&borrow],
        &borrow,
        &collateral,
        350_000_000,
    )
    .unwrap();

    assert_eq!(
        env.token_balance(ata(&liquidator.pubkey(), &collateral.share_mint)),
        1_005_000_000
    );
    assert!(!env.account_is_open(vault));
    assert_eq!(env.sol_balance(obligation), obligation_before + vault_rent);
}

/// A seizure that leaves shares behind leaves the vault open, holding exactly
/// the shares the obligation still records.
#[test]
fn partial_liquidation_keeps_the_vault_open() {
    let (mut env, collateral, borrow, _borrower, obligation, liquidator) = setup();
    env.set_price(collateral.mint, cents(80));
    let vault = env.obligation_share_vault(&collateral, obligation);

    env.try_liquidate(
        &liquidator,
        obligation,
        &[&collateral],
        &[&borrow],
        &borrow,
        &collateral,
        100_000_000,
    )
    .unwrap();

    let seized = env.token_balance(ata(&liquidator.pubkey(), &collateral.share_mint));
    assert!(seized > 0);
    assert!(env.account_is_open(vault));
    let remaining = env.obligation(obligation).deposits[0].deposited_shares;
    assert_eq!(remaining, 1_000_000_000 - seized);
    assert_eq!(env.token_balance(vault), remaining);
}

/// The vault rent a liquidation left in the obligation returns to the owner
/// when they close it: once the remaining debt is repaid, `close_obligation`
/// pays out the obligation's own rent and the vault's together.
#[test]
fn close_obligation_after_full_liquidation_returns_both_rents_to_the_owner() {
    let (mut env, collateral, borrow, borrower, obligation, liquidator) = setup();
    env.set_price(collateral.mint, PRICE_TO_SEIZE_EVERYTHING);
    let vault = env.obligation_share_vault(&collateral, obligation);
    let vault_rent = env.sol_balance(vault);
    let obligation_rent = env.sol_balance(obligation);
    env.try_liquidate(
        &liquidator,
        obligation,
        &[&collateral],
        &[&borrow],
        &borrow,
        &collateral,
        350_000_000,
    )
    .unwrap();

    // The seizure repaid $350 of the $700; the borrower repays the rest.
    env.repay(&borrower, obligation, &borrow, 1_000_000_000);
    assert!(env.obligation(obligation).borrows.is_empty());
    assert_eq!(env.sol_balance(obligation), obligation_rent + vault_rent);

    let fee = env.transaction_fee(&borrower);
    let owner_before = env.sol_balance(borrower.pubkey());
    env.try_close_obligation(&borrower, obligation).unwrap();
    assert!(!env.account_is_open(obligation));
    assert_eq!(
        env.sol_balance(borrower.pubkey()),
        owner_before + obligation_rent + vault_rent - fee,
        "both rents must return to the owner"
    );
}

/// An owner may liquidate their own unhealthy position: they repay their own
/// debt and take their own collateral at the bonus. Pointless economically,
/// but not refused.
#[test]
fn owner_can_liquidate_their_own_obligation() {
    let (mut env, collateral, borrow, borrower, obligation, _liquidator) = setup();
    env.set_price(collateral.mint, cents(80));
    let owner_borrow_tokens = ata(&borrower.pubkey(), &borrow.mint);
    let owner_shares = ata(&borrower.pubkey(), &collateral.share_mint);
    assert_eq!(env.token_balance(owner_borrow_tokens), 700_000_000);
    assert_eq!(env.token_balance(owner_shares), 0);

    env.try_liquidate(
        &borrower,
        obligation,
        &[&collateral],
        &[&borrow],
        &borrow,
        &collateral,
        100_000_000,
    )
    .unwrap();

    assert_eq!(env.token_balance(owner_borrow_tokens), 600_000_000);
    let seized = env.token_balance(owner_shares);
    assert!(seized > 0);
    assert_eq!(
        env.obligation(obligation).deposits[0].deposited_shares,
        1_000_000_000 - seized
    );
}

/// An owner may also liquidate their own position down to nothing: the
/// seizure empties the vault, which closes into the obligation account, while
/// the owner signs as the liquidator. The owner gets every share back and the
/// vault's rent waits in the obligation.
#[test]
fn owner_can_liquidate_their_own_obligation_to_empty() {
    let (mut env, collateral, borrow, borrower, obligation, _liquidator) = setup();
    env.set_price(collateral.mint, PRICE_TO_SEIZE_EVERYTHING);
    let vault = env.obligation_share_vault(&collateral, obligation);
    let vault_rent = env.sol_balance(vault);
    let obligation_before = env.sol_balance(obligation);
    let owner_borrow_tokens = ata(&borrower.pubkey(), &borrow.mint);
    let owner_shares = ata(&borrower.pubkey(), &collateral.share_mint);
    assert_eq!(env.token_balance(owner_borrow_tokens), 700_000_000);
    assert_eq!(env.token_balance(owner_shares), 0);

    env.try_liquidate(
        &borrower,
        obligation,
        &[&collateral],
        &[&borrow],
        &borrow,
        &collateral,
        350_000_000,
    )
    .unwrap();

    assert_eq!(env.token_balance(owner_borrow_tokens), 350_000_000);
    assert_eq!(env.token_balance(owner_shares), 1_000_000_000);
    assert!(env.obligation(obligation).deposits.is_empty());
    assert!(!env.account_is_open(vault));
    assert_eq!(env.sol_balance(obligation), obligation_before + vault_rent);
}

/// A liquidation that takes all the collateral and half the debt leaves the
/// obligation holding debt and nothing else, and `close_obligation` refuses it
/// until that debt is repaid.
#[test]
fn close_obligation_refused_while_debt_remains_after_full_liquidation() {
    let (mut env, collateral, borrow, borrower, obligation, liquidator) = setup();
    env.set_price(collateral.mint, PRICE_TO_SEIZE_EVERYTHING);
    env.try_liquidate(
        &liquidator,
        obligation,
        &[&collateral],
        &[&borrow],
        &borrow,
        &collateral,
        350_000_000,
    )
    .unwrap();
    let state = env.obligation(obligation);
    assert!(state.deposits.is_empty());
    assert_eq!(state.borrows.len(), 1);

    let result = env.try_close_obligation(&borrower, obligation);
    common::assert_program_error!(result, LendingError::ObligationNotEmpty);
    assert!(env.account_is_open(obligation));
}

/// After a liquidation closes the vault, the owner can post collateral again:
/// the deposit recreates the vault, paid for by the owner, and the old vault's
/// rent stays in the obligation.
#[test]
fn redeposit_after_full_liquidation_recreates_the_vault() {
    let (mut env, collateral, borrow, borrower, obligation, liquidator) = setup();
    env.set_price(collateral.mint, PRICE_TO_SEIZE_EVERYTHING);
    let vault = env.obligation_share_vault(&collateral, obligation);
    let vault_rent = env.sol_balance(vault);
    let obligation_rent = env.sol_balance(obligation);
    env.try_liquidate(
        &liquidator,
        obligation,
        &[&collateral],
        &[&borrow],
        &borrow,
        &collateral,
        350_000_000,
    )
    .unwrap();
    assert!(!env.account_is_open(vault));

    // The owner has no shares left, so the liquidator hands 600 back.
    let owner_shares = ata(&borrower.pubkey(), &collateral.share_mint);
    env.send_shares(&liquidator, &collateral, owner_shares, 600_000_000);
    env.post_collateral(&borrower, obligation, &collateral, 600_000_000);

    assert!(env.account_is_open(vault));
    assert_eq!(env.sol_balance(vault), vault_rent);
    assert_eq!(env.token_balance(vault), 600_000_000);
    assert_eq!(env.token_balance(owner_shares), 0);
    assert_eq!(
        env.obligation(obligation).deposits[0].deposited_shares,
        600_000_000
    );
    assert_eq!(env.sol_balance(obligation), obligation_rent + vault_rent);
}
