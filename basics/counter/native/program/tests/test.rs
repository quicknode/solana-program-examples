use borsh::BorshDeserialize;
use counter_solana_native::Counter;
use litesvm::LiteSVM;
use solana_instruction::{AccountMeta, Instruction};
use solana_keypair::{Keypair, Signer};
use solana_native_token::LAMPORTS_PER_SOL;
use solana_pubkey::Pubkey;
use solana_rent::Rent;
use solana_system_interface::instruction::create_account;
use solana_transaction::Transaction;

// The .so is built into ../../tests/fixtures by `pnpm build-and-test` (which runs
// `cargo build-sbf --sbf-out-dir=./tests/fixtures` from the package root). Run
// that script (or `cargo build-sbf` with --sbf-out-dir set accordingly) before
// `cargo test`.
const PROGRAM_SO: &[u8] = include_bytes!("../../tests/fixtures/counter_solana_native.so");

fn setup_with_counter() -> (LiteSVM, Pubkey, Keypair, Keypair) {
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
    (svm, program_id, payer, counter_account)
}

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

    // Actually assert the transaction succeeded - the previous shape used
    // `let _ = ...is_ok();` which discarded the result.
    svm.send_transaction(tx).unwrap();

    let counter_account_data = svm.get_account(&counter_account.pubkey()).unwrap().data;
    let counter = Counter::try_from_slice(&counter_account_data).unwrap();
    assert_eq!(counter.count, 1);
}

#[test]
fn test_unknown_instruction_fails() {
    let (mut svm, program_id, payer, counter_account) = setup_with_counter();

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

#[test]
fn test_wrong_owner_fails() {
    let program_id = Pubkey::new_unique();

    let mut svm = LiteSVM::new();
    svm.add_program(program_id, PROGRAM_SO).unwrap();

    let payer = Keypair::new();
    let counter_account = Keypair::new();
    svm.airdrop(&payer.pubkey(), LAMPORTS_PER_SOL * 10).unwrap();

    // Create the counter account but owned by a different (random) program.
    let counter_account_size = std::mem::size_of::<Counter>();
    let wrong_owner = Pubkey::new_unique();
    let create_ix = create_account(
        &payer.pubkey(),
        &counter_account.pubkey(),
        Rent::default().minimum_balance(counter_account_size),
        counter_account_size as u64,
        &wrong_owner,
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
        &[&payer],
        svm.latest_blockhash(),
    );
    assert!(
        svm.send_transaction(tx).is_err(),
        "counter owned by a different program must be rejected"
    );
}
