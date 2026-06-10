use litesvm::LiteSVM;
use solana_instruction::{AccountMeta, Instruction};
use solana_keypair::{Keypair, Signer};
use solana_program::native_token::LAMPORTS_PER_SOL;
use solana_pubkey::Pubkey;
use solana_system_interface::instruction::create_account;
use solana_transaction::Transaction;
use transfer_sol_program::processor::TransferInstruction;

#[test]
fn test_transfer_sol() {
    let mut svm = LiteSVM::new();

    let program_id = Pubkey::new_unique();
    let program_bytes = include_bytes!("../../tests/fixtures/transfer_sol_program.so");

    svm.add_program(program_id, program_bytes).unwrap();

    let payer = Keypair::new();
    svm.airdrop(&payer.pubkey(), LAMPORTS_PER_SOL * 10).unwrap();

    let test_recipient1 = Keypair::new();
    let test_recipient2 = Keypair::new();
    let test_recipient3 = Keypair::new();

    let payer_balance_before = svm.get_balance(&payer.pubkey()).unwrap();
    let recipient_balance_before = svm.get_balance(&test_recipient1.pubkey()).unwrap_or(0);

    let data = borsh::to_vec(&TransferInstruction::CpiTransfer(LAMPORTS_PER_SOL)).unwrap();

    let ix = Instruction {
        program_id,
        accounts: vec![
            AccountMeta::new(payer.pubkey(), true),
            AccountMeta::new(test_recipient1.pubkey(), false),
            AccountMeta::new(solana_system_interface::program::ID, false),
        ],
        data,
    };

    let tx = Transaction::new_signed_with_payer(
        &[ix],
        Some(&payer.pubkey()),
        &[&payer],
        svm.latest_blockhash(),
    );

    assert!(svm.send_transaction(tx).is_ok());

    let payer_balance_after = svm.get_balance(&payer.pubkey()).unwrap();
    let recipient_balance_after = svm.get_balance(&test_recipient1.pubkey()).unwrap_or(0);

    assert!(payer_balance_before > payer_balance_after);
    assert!(recipient_balance_before < recipient_balance_after);

    let create_ix = create_account(
        &payer.pubkey(),
        &test_recipient2.pubkey(),
        2 * LAMPORTS_PER_SOL,
        0,
        &program_id,
    );

    let tx = Transaction::new_signed_with_payer(
        &[create_ix],
        Some(&payer.pubkey()),
        &[&payer, &test_recipient2],
        svm.latest_blockhash(),
    );

    assert!(svm.send_transaction(tx).is_ok());

    let data = borsh::to_vec(&TransferInstruction::ProgramTransfer(LAMPORTS_PER_SOL)).unwrap();

    let ix = Instruction {
        program_id,
        accounts: vec![
            AccountMeta::new(test_recipient2.pubkey(), true),
            AccountMeta::new(test_recipient3.pubkey(), false),
            AccountMeta::new(solana_system_interface::program::ID, false),
        ],
        data,
    };

    let tx = Transaction::new_signed_with_payer(
        &[ix],
        Some(&payer.pubkey()),
        &[&payer, &test_recipient2],
        svm.latest_blockhash(),
    );

    assert!(svm.send_transaction(tx).is_ok());
}

#[test]
fn test_program_transfer_rejects_insufficient_funds() {
    let mut svm = LiteSVM::new();

    let program_id = Pubkey::new_unique();
    let program_bytes = include_bytes!("../../tests/fixtures/transfer_sol_program.so");
    svm.add_program(program_id, program_bytes).unwrap();

    let payer = Keypair::new();
    svm.airdrop(&payer.pubkey(), LAMPORTS_PER_SOL * 10).unwrap();

    // A program-owned account holding 1 SOL.
    let program_owned_account = Keypair::new();
    let recipient = Keypair::new();
    let create_ix = create_account(
        &payer.pubkey(),
        &program_owned_account.pubkey(),
        LAMPORTS_PER_SOL,
        0,
        &program_id,
    );
    let tx = Transaction::new_signed_with_payer(
        &[create_ix],
        Some(&payer.pubkey()),
        &[&payer, &program_owned_account],
        svm.latest_blockhash(),
    );
    assert!(svm.send_transaction(tx).is_ok());

    // Ask for more than the account holds: the checked subtraction must
    // reject the transfer instead of wrapping.
    let data = borsh::to_vec(&TransferInstruction::ProgramTransfer(2 * LAMPORTS_PER_SOL)).unwrap();
    let ix = Instruction {
        program_id,
        accounts: vec![
            AccountMeta::new(program_owned_account.pubkey(), false),
            AccountMeta::new(recipient.pubkey(), false),
        ],
        data,
    };
    let tx = Transaction::new_signed_with_payer(
        &[ix],
        Some(&payer.pubkey()),
        &[&payer],
        svm.latest_blockhash(),
    );
    assert!(
        svm.send_transaction(tx).is_err(),
        "overdrawing the payer must fail"
    );

    // Balances are untouched.
    assert_eq!(
        svm.get_balance(&program_owned_account.pubkey()).unwrap(),
        LAMPORTS_PER_SOL
    );
    assert_eq!(svm.get_balance(&recipient.pubkey()).unwrap_or(0), 0);
}
