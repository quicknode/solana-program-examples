use {crate::cpi::CreateSystemAccountInstruction, quasar_test::prelude::*};

// Deterministic addresses keep tests independent of discovery order.
const PAYER: Pubkey = Pubkey::new_from_array([1; 32]);
const NEW_ACCOUNT: Pubkey = Pubkey::new_from_array([2; 32]);

#[quasar_test]
fn create_system_account_funds_a_rent_exempt_account(test: &mut Test) {
    test.add(Wallet::new().at(PAYER));
    // NEW_ACCOUNT stays absent: a missing writable account enters the
    // transaction as an empty system account, the exact shape the system
    // program's create_account CPI expects.

    // The system program is a canonical derivation, so the generated
    // instruction only asks for the two signers.
    test.send(CreateSystemAccountInstruction {
        payer: PAYER,
        new_account: NEW_ACCOUNT,
    })
    .succeeds();

    // Verify the new account exists and is owned by the system program.
    let account = test.account(NEW_ACCOUNT).unwrap();
    assert_eq!(
        account.owner,
        system_program::ID,
        "account should be system-owned"
    );
    assert!(
        account.lamports > 0,
        "account should have rent-exempt lamports"
    );
    assert_eq!(account.data.len(), 0, "account should have zero data");
}
