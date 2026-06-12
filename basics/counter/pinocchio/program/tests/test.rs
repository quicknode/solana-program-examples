use counter_solana_pinocchio::Counter;
use litesvm::LiteSVM;
use solana_instruction::{AccountMeta, Instruction};
use solana_keypair::{Keypair, Signer};
use solana_native_token::LAMPORTS_PER_SOL;
use solana_pubkey::Pubkey;
use solana_rent::Rent;
use solana_system_interface::instruction::create_account;
use solana_transaction::Transaction;

// The .so is built into the workspace target/deploy by
// `cargo build-sbf --manifest-path=./program/Cargo.toml` (run from the project
// root). Rebuild after every program change: the binary is embedded at
// test-compile time, so a stale .so silently tests old code.
const PROGRAM_SO: &[u8] = include_bytes!("../../../../../target/deploy/counter_solana_pinocchio.so");

#[test]
fn test_counter() {
    let program_id = Pubkey::new_unique();

    let mut svm = LiteSVM::new();
    svm.add_program(program_id, PROGRAM_SO).unwrap();

    let payer = Keypair::new();
    let counter_account = Keypair::new();

    svm.airdrop(&payer.pubkey(), LAMPORTS_PER_SOL * 10).unwrap();

    let counter_account_size = std::mem::size_of::<Counter>();

    let create_ix = create_account(
        &payer.pubkey(),
        &counter_account.pubkey(),
        Rent::default().minimum_balance(counter_account_size),
        counter_account_size as u64,
        &program_id,
    );

    let tx = Transaction::new_signed_with_payer(
        &[create_ix],
        Some(&payer.pubkey()),
        &[&payer, &counter_account],
        svm.latest_blockhash(),
    );

    svm.send_transaction(tx).unwrap();

    let ix = Instruction {
        program_id,
        accounts: vec![AccountMeta::new(counter_account.pubkey(), false)],
        data: vec![0],
    };

    let tx = Transaction::new_signed_with_payer(
        &[ix],
        Some(&payer.pubkey()),
        &[payer],
        svm.latest_blockhash(),
    );

    svm.send_transaction(tx).unwrap();

    let counter_account_data = svm.get_account(&counter_account.pubkey()).unwrap().data;
    let counter_bytes: [u8; 8] = counter_account_data[0..8].try_into().unwrap();
    let count = u64::from_le_bytes(counter_bytes);
    assert_eq!(count, 1);
}

#[test]
fn test_unknown_instruction_fails() {
    let program_id = Pubkey::new_unique();

    let mut svm = LiteSVM::new();
    svm.add_program(program_id, PROGRAM_SO).unwrap();

    let payer = Keypair::new();
    let counter_account = Keypair::new();
    svm.airdrop(&payer.pubkey(), LAMPORTS_PER_SOL * 10).unwrap();

    let counter_account_size = std::mem::size_of::<Counter>();
    let create_ix = create_account(
        &payer.pubkey(),
        &counter_account.pubkey(),
        Rent::default().minimum_balance(counter_account_size),
        counter_account_size as u64,
        &program_id,
    );
    let tx = Transaction::new_signed_with_payer(
        &[create_ix],
        Some(&payer.pubkey()),
        &[&payer, &counter_account],
        svm.latest_blockhash(),
    );
    svm.send_transaction(tx).unwrap();

    // Discriminant 9 is not handled and must now return an error rather than Ok(()).
    let ix = Instruction {
        program_id,
        accounts: vec![AccountMeta::new(counter_account.pubkey(), false)],
        data: vec![9],
    };
    let tx = Transaction::new_signed_with_payer(
        &[ix],
        Some(&payer.pubkey()),
        &[&payer],
        svm.latest_blockhash(),
    );
    assert!(
        svm.send_transaction(tx).is_err(),
        "unknown instruction discriminant must fail"
    );
}
