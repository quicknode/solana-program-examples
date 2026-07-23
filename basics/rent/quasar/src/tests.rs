use {crate::cpi::CreateSystemAccountInstruction, quasar_test::prelude::*};

// Deterministic addresses keep tests independent of discovery order.
const PAYER: Pubkey = Pubkey::new_from_array([1; 32]);
// The new account is a fresh keypair whose address the caller chooses.
const NEW_ACCOUNT: Pubkey = Pubkey::new_from_array([2; 32]);

#[quasar_test]
fn create_system_account_sized_for_address_data(test: &mut Test) {
    test.add(Wallet::new().at(PAYER));

    let name = "Joe C";
    let address = "123 Main St";

    let outcome = test.send(CreateSystemAccountInstruction {
        payer: PAYER,
        new_account: NEW_ACCOUNT,
        name: name.to_string().into(),
        address: address.to_string().into(),
    });
    outcome.succeeds();

    // Verify the account was created with the expected data size:
    // borsh-style 4-byte length prefix + bytes for each String field.
    let account = test.account(NEW_ACCOUNT).unwrap();
    let expected_space = 4 + name.len() + 4 + address.len();
    assert_eq!(
        account.data.len(),
        expected_space,
        "account data should be sized for the address data"
    );
    assert!(account.lamports > 0, "account should have rent-exempt lamports");

    let logs = outcome.logs().join("\n");
    assert!(logs.contains("Creating a system account"), "should log creation");
    assert!(logs.contains("Account created successfully"), "should log success");
}
