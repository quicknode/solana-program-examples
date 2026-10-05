mod common;

use anchor_v2_testing::{Keypair, Signer};
use lending::errors::LendingError;

use common::{ata, default_config, dollars, Env, ReserveHandle};

/// A collateral reserve, a funded borrow reserve, and a borrower with an
/// obligation holding 1000 units of collateral and no debt.
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

    let supplier = env.create_user();
    env.fund(&supplier, borrow.mint, 1_000_000_000);
    env.supply(&supplier, &borrow, 1_000_000_000);

    let borrower = env.create_user();
    env.fund(&borrower, collateral.mint, 1_000_000_000);
    env.fund(&borrower, borrow.mint, 0);
    env.supply(&borrower, &collateral, 1_000_000_000);
    let obligation = env.initialize_obligation(&borrower);
    env.post_collateral(&borrower, obligation, &collateral, 1_000_000_000);

    (env, collateral, borrow, borrower, obligation)
}

/// Once the collateral is out, closing the obligation returns its rent to the
/// owner, less the transaction fee, and the account is gone.
#[test]
fn close_obligation_returns_rent_to_owner() {
    let (mut env, collateral, _borrow, borrower, obligation) = setup();
    env.try_withdraw_collateral_without_refresh(&borrower, obligation, &collateral, 1_000_000_000)
        .unwrap();
    assert!(env.obligation(obligation).deposits.is_empty());

    let fee = env.transaction_fee(&borrower);
    let rent = env.sol_balance(obligation);
    assert!(rent > 0);
    let owner_before = env.sol_balance(borrower.pubkey());

    env.try_close_obligation(&borrower, obligation).unwrap();
    assert!(!env.account_is_open(obligation));
    assert_eq!(
        env.sol_balance(borrower.pubkey()),
        owner_before + rent - fee,
        "the obligation's rent must return to its owner"
    );
}

/// An obligation still holding collateral cannot close: the shares would be
/// stranded in a vault whose authority no longer exists.
#[test]
fn close_obligation_with_collateral_is_refused() {
    let (mut env, collateral, _borrow, borrower, obligation) = setup();
    let result = env.try_close_obligation(&borrower, obligation);
    common::assert_program_error!(result, LendingError::ObligationNotEmpty);
    assert!(env.account_is_open(obligation));
    assert_eq!(
        env.token_balance(env.obligation_share_vault(&collateral, obligation)),
        1_000_000_000
    );
}

/// An obligation with debt cannot close: closing it would forgive the loan.
#[test]
fn close_obligation_with_debt_is_refused() {
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
    let result = env.try_close_obligation(&borrower, obligation);
    common::assert_program_error!(result, LendingError::ObligationNotEmpty);
    assert!(env.account_is_open(obligation));
    assert_eq!(env.obligation(obligation).borrows.len(), 1);
}

/// Only the owner may close their obligation and take its rent; `address =
/// obligation.owner` rejects anyone else before the handler runs. That is
/// Anchor's constraint error, not one of the program's; v2 logs only its
/// numeric code.
#[test]
fn non_owner_cannot_close_obligation() {
    let (mut env, collateral, _borrow, borrower, obligation) = setup();
    env.try_withdraw_collateral_without_refresh(&borrower, obligation, &collateral, 1_000_000_000)
        .unwrap();

    let stranger = env.create_user();
    let result = env.try_close_obligation(&stranger, obligation);
    let anchor_lang::Error::Custom(code) =
        anchor_lang::Error::from(anchor_lang::ErrorCode::ConstraintAddress)
    else {
        panic!("a constraint error converts to a custom code");
    };
    let message = result.expect_err("only the owner may close their obligation");
    assert!(
        message.contains(&format!("Custom({code})")),
        "expected ConstraintAddress (Custom({code})), got: {message}"
    );
    assert!(env.account_is_open(obligation));
    // The owner's share account is untouched either way.
    assert_eq!(
        env.token_balance(ata(&borrower.pubkey(), &collateral.share_mint)),
        1_000_000_000
    );
}

/// A withdrawal that takes the last share closes the collateral vault and
/// returns its rent to the owner, who paid it when the first deposit created
/// the vault.
#[test]
fn full_withdraw_closes_the_vault_and_returns_its_rent() {
    let (mut env, collateral, _borrow, borrower, obligation) = setup();
    let vault = env.obligation_share_vault(&collateral, obligation);
    let vault_rent = env.sol_balance(vault);
    assert!(vault_rent > 0);
    let fee = env.transaction_fee(&borrower);
    let owner_before = env.sol_balance(borrower.pubkey());

    env.try_withdraw_collateral_without_refresh(&borrower, obligation, &collateral, 1_000_000_000)
        .unwrap();
    assert!(!env.account_is_open(vault));
    assert_eq!(
        env.sol_balance(borrower.pubkey()),
        owner_before + vault_rent - fee,
        "the vault's rent must return to the owner"
    );
    assert_eq!(
        env.token_balance(ata(&borrower.pubkey(), &collateral.share_mint)),
        1_000_000_000
    );
}

/// A withdrawal that leaves shares behind leaves the vault open.
#[test]
fn partial_withdraw_keeps_the_vault_open() {
    let (mut env, collateral, _borrow, borrower, obligation) = setup();
    let vault = env.obligation_share_vault(&collateral, obligation);
    env.try_withdraw_collateral_without_refresh(&borrower, obligation, &collateral, 400_000_000)
        .unwrap();
    assert!(env.account_is_open(vault));
    assert_eq!(env.token_balance(vault), 600_000_000);
    assert_eq!(
        env.obligation(obligation).deposits[0].deposited_shares,
        600_000_000
    );
}

/// The deposit handler creates the vault with `init_if_needed`, so posting
/// into the same reserve after a full withdrawal recreates it, and the
/// position works as before.
#[test]
fn redeposit_after_full_withdraw_recreates_the_vault() {
    let (mut env, collateral, borrow, borrower, obligation) = setup();
    let vault = env.obligation_share_vault(&collateral, obligation);
    env.try_withdraw_collateral_without_refresh(&borrower, obligation, &collateral, 1_000_000_000)
        .unwrap();
    assert!(!env.account_is_open(vault));

    env.post_collateral(&borrower, obligation, &collateral, 600_000_000);
    assert!(env.account_is_open(vault));
    assert_eq!(env.token_balance(vault), 600_000_000);
    assert_eq!(
        env.obligation(obligation).deposits[0].deposited_shares,
        600_000_000
    );

    // The recreated vault backs a borrow like the first one did.
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

/// Share tokens sent straight to a vault are not recorded in the obligation,
/// so they could otherwise leave it holding a balance when the last recorded
/// share comes out, and an account holding tokens cannot close. The emptying
/// withdrawal sweeps the whole balance to the owner, so the vault still
/// closes and the withdrawal still succeeds.
#[test]
fn donated_shares_cannot_keep_the_vault_open() {
    let (mut env, collateral, _borrow, borrower, obligation) = setup();
    let vault = env.obligation_share_vault(&collateral, obligation);

    let donor = env.create_user();
    env.fund(&donor, collateral.mint, 5_000_000);
    env.supply(&donor, &collateral, 5_000_000);
    env.send_shares(&donor, &collateral, vault, 5_000_000);
    assert_eq!(env.token_balance(vault), 1_005_000_000);

    env.try_withdraw_collateral_without_refresh(&borrower, obligation, &collateral, 1_000_000_000)
        .unwrap();
    assert!(!env.account_is_open(vault));
    assert_eq!(
        env.token_balance(ata(&borrower.pubkey(), &collateral.share_mint)),
        1_005_000_000
    );
}
