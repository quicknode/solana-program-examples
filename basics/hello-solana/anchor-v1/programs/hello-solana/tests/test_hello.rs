mod transaction_v1;

use transaction_v1::v1_transaction;

use {
    anchor_lang::{solana_program::instruction::Instruction, InstructionData, ToAccountMetas},
    litesvm::LiteSVM,
    solana_kite::create_wallet,
    solana_signer::Signer,
};

#[test]
fn test_say_hello() {
    let program_id = hello_solana::id();
    let mut svm = LiteSVM::new();
    let bytes = include_bytes!("../../../target/deploy/hello_solana.so");
    svm.add_program(program_id, bytes).unwrap();
    let payer = create_wallet(&mut svm, 1_000_000_000).unwrap();

    let instruction = Instruction::new_with_bytes(
        program_id,
        &hello_solana::instruction::Hello {}.data(),
        hello_solana::accounts::HelloAccountConstraints {}.to_account_metas(None),
    );

    // The program only logs; assert it emitted its greeting rather than merely
    // that the transaction landed. The send helper discards the metadata, so
    // send through LiteSVM directly to read the logs.
    let transaction = v1_transaction(&svm, &[instruction], &[&payer], &payer.pubkey());
    let metadata = svm.send_transaction(transaction).unwrap();

    assert!(
        metadata
            .logs
            .iter()
            .any(|log| log.contains("Hello, Solana!")),
        "expected the program to log its greeting, got: {:?}",
        metadata.logs
    );
}
