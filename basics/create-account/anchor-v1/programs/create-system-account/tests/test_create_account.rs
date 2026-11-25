use {
    anchor_lang::{
        solana_program::{instruction::Instruction, system_program},
        InstructionData, ToAccountMetas,
    },
    litesvm::LiteSVM,
    solana_keypair::Keypair,
    solana_kite::{create_wallet, send_transaction_from_instructions},
    solana_rent::Rent,
    solana_signer::Signer,
};

#[test]
fn test_create_the_account() {
    let program_id = create_system_account::id();
    let mut svm = LiteSVM::new();
    let bytes = include_bytes!("../../../target/deploy/create_system_account.so");
    svm.add_program(program_id, bytes).unwrap();
    let payer = create_wallet(&mut svm, 10_000_000_000).unwrap();

    let new_account = Keypair::new();

    let instruction = Instruction::new_with_bytes(
        program_id,
        &create_system_account::instruction::CreateSystemAccount {}.data(),
        create_system_account::accounts::CreateSystemAccountAccountConstraints {
            payer: payer.pubkey(),
            new_account: new_account.pubkey(),
            system_program: system_program::id(),
        }
        .to_account_metas(None),
    );

    send_transaction_from_instructions(
        &mut svm,
        vec![instruction],
        &[&payer, &new_account],
        &payer.pubkey(),
        None,
    )
    .unwrap();

    // Minimum balance for rent exemption for a 0-data account. Read the rent
    // sysvar out of the SVM rather than using `Rent::default()`: the program
    // under test calls `Rent::get()`, so the runtime's rent is the only value
    // it can produce, and the two rates diverged when LiteSVM moved from the
    // Agave 4.2 crates to 4.3.
    let rent: Rent = svm.get_sysvar();
    let lamports = rent.minimum_balance(0);

    let account_info = svm.get_account(&new_account.pubkey()).unwrap();
    assert_eq!(account_info.lamports, lamports);
}
