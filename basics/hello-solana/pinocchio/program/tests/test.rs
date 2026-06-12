use litesvm::LiteSVM;
use solana_instruction::{AccountMeta, Instruction};
use solana_keypair::{Keypair, Signer};
use solana_native_token::LAMPORTS_PER_SOL;
use solana_pubkey::Pubkey;
use solana_transaction::Transaction;

// The .so is built into the workspace target/deploy by
// `cargo build-sbf --manifest-path=./program/Cargo.toml` (run from the project
// root). Rebuild after every program change: the binary is embedded at
// test-compile time, so a stale .so silently tests old code.
const PROGRAM_SO: &[u8] =
    include_bytes!("../../../../../target/deploy/hello_solana_program_pinocchio.so");

#[test]
fn test_hello_solana() {
    let program_id = Pubkey::new_unique();

    let mut svm = LiteSVM::new();
    svm.add_program(program_id, PROGRAM_SO).unwrap();

    let payer = Keypair::new();
    svm.airdrop(&payer.pubkey(), LAMPORTS_PER_SOL * 10).unwrap();

    let ix = Instruction {
        program_id,
        accounts: vec![AccountMeta::new(payer.pubkey(), true)],
        data: vec![0],
    };

    let tx = Transaction::new_signed_with_payer(
        &[ix],
        Some(&payer.pubkey()),
        &[&payer],
        svm.latest_blockhash(),
    );

    let result = svm.send_transaction(tx);
    assert!(result.is_ok());

    let logs = result.unwrap().logs;
    assert!(logs.iter().any(|log| log.contains("Hello, Solana!")));
}
