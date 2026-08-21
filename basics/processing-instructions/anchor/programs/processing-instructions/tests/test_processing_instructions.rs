use {
    anchor_lang::{solana_program::instruction::Instruction, InstructionData, ToAccountMetas},
    anchor_v2_testing::{LiteSVM, Signer},
    solana_kite::{create_wallet, send_transaction_from_instructions},
};

fn setup() -> (LiteSVM, anchor_v2_testing::Keypair) {
    let program_id = processing_instructions::id();
    let mut svm = anchor_v2_testing::svm();
    let bytes = include_bytes!("../../../target/deploy/processing_instructions.so");
    svm.add_program(program_id, bytes).unwrap();
    let payer = create_wallet(&mut svm, 10_000_000_000).unwrap();
    (svm, payer)
}

#[test]
fn test_go_to_park() {
    let (mut svm, payer) = setup();
    let program_id = processing_instructions::id();

    // Test with short person (height 3)
    let ix_short = Instruction::new_with_bytes(
        program_id,
        &processing_instructions::instruction::GoToPark {
            name: "Jimmy".to_string(),
            height: 3,
        }
        .data(),
        processing_instructions::accounts::ParkAccountConstraints {}.to_account_metas(None),
    );
    send_transaction_from_instructions(&mut svm, vec![ix_short], &[&payer], &payer.pubkey())
        .unwrap();

    svm.expire_blockhash();

    // Test with tall person (height 10)
    let ix_tall = Instruction::new_with_bytes(
        program_id,
        &processing_instructions::instruction::GoToPark {
            name: "Mary".to_string(),
            height: 10,
        }
        .data(),
        processing_instructions::accounts::ParkAccountConstraints {}.to_account_metas(None),
    );
    send_transaction_from_instructions(&mut svm, vec![ix_tall], &[&payer], &payer.pubkey())
        .unwrap();
}
