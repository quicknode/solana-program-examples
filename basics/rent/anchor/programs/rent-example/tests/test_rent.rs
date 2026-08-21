use {
    anchor_lang::{
    anchor_v2_testing::{Keypair, LiteSVM, Signer},
        solana_program::instruction::Instruction, system_program, InstructionData, ToAccountMetas,
    },
    solana_kite::{create_wallet, send_transaction_from_instructions},
};

#[test]
fn test_create_system_account() {
    let program_id = rent_example::id();
    let mut svm = anchor_v2_testing::svm();
    let bytes = include_bytes!("../../../target/deploy/rent_example.so");
    svm.add_program(program_id, bytes).unwrap();
    let payer = create_wallet(&mut svm, 10_000_000_000).unwrap();

    let new_account = Keypair::new();

    let name = "Marcus";
    let address = "123 Main St. San Francisco, CA";

    let ix_data = rent_example::instruction::CreateSystemAccount {
        address_data: rent_example::AddressData {
            name: name.to_string(),
            address: address.to_string(),
        },
    }
    .data();

    let instruction = Instruction::new_with_bytes(
        program_id,
        &ix_data,
        rent_example::accounts::CreateSystemAccountAccountConstraints {
            payer: payer.pubkey(),
            new_account: new_account.pubkey(),
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    );

    send_transaction_from_instructions(
        &mut svm,
        vec![instruction],
        &[&payer, &new_account],
        &payer.pubkey(),
    )
    .unwrap();

    // Verify the account was created with the correct size
    // Serialized AddressData: 4 + 6 ("Marcus") + 4 + 30 = 44 bytes
    let expected_size = 4 + name.len() + 4 + address.len();
    let account = svm.get_account(&new_account.pubkey()).unwrap();
    assert_eq!(account.data.len(), expected_size);
    assert!(
        account.lamports > 0,
        "Account should have lamports for rent"
    );
}
