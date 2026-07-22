use {
    crate::{
        cpi::{CreateNewAccountInstruction, InitRentVaultInstruction},
        instructions::init_rent_vault::RentVault,
    },
    quasar_test::prelude::*,
};

// Deterministic addresses keep tests independent of discovery order.
const PAYER: Pubkey = Pubkey::new_from_array([1; 32]);
const NEW_ACCOUNT: Pubkey = Pubkey::new_from_array([2; 32]);

const FUND_AMOUNT: u64 = 5_000_000_000; // 5 SOL

#[quasar_test]
fn init_rent_vault_funds_the_vault_pda(test: &mut Test) {
    test.add(Wallet::new().at(PAYER));
    let rent_vault = test.derive_pda(RentVault::seeds());

    // The rent-vault PDA and system program are canonical derivations, so
    // the generated instruction only asks for the payer and the amount.
    test.send(InitRentVaultInstruction {
        payer: PAYER,
        fund_lamports: FUND_AMOUNT,
    })
    .succeeds()
    .has_lamports(rent_vault, FUND_AMOUNT);
}

#[quasar_test]
fn create_new_account_is_paid_for_by_the_vault(test: &mut Test) {
    test.add(Wallet::new().at(PAYER));
    let rent_vault = test.derive_pda(RentVault::seeds());

    // Step 1: fund the rent vault.
    test.send(InitRentVaultInstruction {
        payer: PAYER,
        fund_lamports: FUND_AMOUNT,
    })
    .succeeds();

    // Step 2: create a new account funded by the vault. NEW_ACCOUNT stays
    // absent: a missing writable account enters the transaction as an empty
    // system account, which is the "not yet created" signer shape the
    // create_account CPI expects.
    test.send(CreateNewAccountInstruction {
        new_account: NEW_ACCOUNT,
    })
    .succeeds();

    // Verify the new account was created.
    let new_account = test.account(NEW_ACCOUNT).unwrap();
    assert_eq!(
        new_account.owner,
        system_program::ID,
        "new account should be system-owned"
    );
    assert!(
        new_account.lamports > 0,
        "new account should have rent-exempt lamports"
    );
    assert_eq!(
        new_account.data.len(),
        0,
        "new account should have zero data"
    );

    // Verify the vault paid for it.
    assert_eq!(
        test.lamports(rent_vault),
        FUND_AMOUNT - new_account.lamports,
        "vault should have paid the new account's rent"
    );
}
