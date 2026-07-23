use {crate::cpi::CheckAccountsInstruction, quasar_test::prelude::*};

// Deterministic addresses keep tests independent of discovery order.
const PAYER: Pubkey = Pubkey::new_from_array([1; 32]);
const ACCOUNT_TO_CREATE: Pubkey = Pubkey::new_from_array([2; 32]);
const ACCOUNT_TO_CHANGE: Pubkey = Pubkey::new_from_array([3; 32]);

#[quasar_test]
fn check_accounts_accepts_a_valid_account_set(test: &mut Test) {
    test.add(Wallet::new().at(PAYER));
    // The account to create stays absent: a missing writable account enters
    // the transaction as an empty system account, exactly the "not yet
    // created" shape this instruction expects.
    // The account to change already exists and is owned by this program.
    test.set_account(Account::new(
        ACCOUNT_TO_CHANGE,
        crate::ID,
        1_000_000,
        vec![0u8; 32],
    ));

    // The system program is a canonical derivation, so the generated
    // instruction only asks for the caller-controlled accounts.
    test.send(CheckAccountsInstruction {
        payer: PAYER,
        account_to_create: ACCOUNT_TO_CREATE,
        account_to_change: ACCOUNT_TO_CHANGE,
    })
    .succeeds();
}
