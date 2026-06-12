use litesvm::LiteSVM;
use solana_instruction::{AccountMeta, Instruction};
use solana_keypair::{Keypair, Signer};
use solana_native_token::LAMPORTS_PER_SOL;
use solana_pubkey::Pubkey;
use solana_transaction::Transaction;

// The .so is built into ../../tests/fixtures by `pnpm build-and-test` (which runs
// `cargo build-sbf --sbf-out-dir=./tests/fixtures` from the package root). Run
// that script (or `cargo build-sbf` with --sbf-out-dir set accordingly) before
// `cargo test`.
const PROGRAM_SO: &[u8] =
    include_bytes!("../../tests/fixtures/repository_layout_pinocchio_program.so");

// Builds the carnival instruction data in the wire format the program decodes:
//   name (str), height (u32), ticket_count (u32), attraction (str), attraction_name (str)
// where each str is a u32 LE length followed by its utf-8 bytes.
fn carnival_ix_data(
    name: &str,
    height: u32,
    ticket_count: u32,
    attraction: &str,
    attraction_name: &str,
) -> Vec<u8> {
    fn push_str(buf: &mut Vec<u8>, s: &str) {
        buf.extend_from_slice(&(s.len() as u32).to_le_bytes());
        buf.extend_from_slice(s.as_bytes());
    }

    let mut data = Vec::new();
    push_str(&mut data, name);
    data.extend_from_slice(&height.to_le_bytes());
    data.extend_from_slice(&ticket_count.to_le_bytes());
    push_str(&mut data, attraction);
    push_str(&mut data, attraction_name);
    data
}

fn setup() -> (LiteSVM, Pubkey, Keypair) {
    let program_id = Pubkey::new_unique();

    let mut svm = LiteSVM::new();
    svm.add_program(program_id, PROGRAM_SO).unwrap();

    let payer = Keypair::new();
    svm.airdrop(&payer.pubkey(), LAMPORTS_PER_SOL * 10).unwrap();

    (svm, program_id, payer)
}

// The program ignores accounts entirely, so a single signer is all we need.
fn send_carnival(svm: &mut LiteSVM, program_id: Pubkey, payer: &Keypair, data: Vec<u8>) -> bool {
    let ix = Instruction {
        program_id,
        accounts: vec![AccountMeta::new(payer.pubkey(), true)],
        data,
    };
    let tx = Transaction::new_signed_with_payer(
        &[ix],
        Some(&payer.pubkey()),
        &[payer],
        svm.latest_blockhash(),
    );
    svm.send_transaction(tx).is_ok()
}

#[test]
fn test_go_on_a_ride() {
    let (mut svm, program_id, payer) = setup();
    assert!(send_carnival(
        &mut svm,
        program_id,
        &payer,
        carnival_ix_data("Alice", 56, 15, "ride", "Scrambler")
    ));
}

#[test]
fn test_play_a_game() {
    let (mut svm, program_id, payer) = setup();
    assert!(send_carnival(
        &mut svm,
        program_id,
        &payer,
        carnival_ix_data("Bob", 49, 6, "game", "Ring Toss")
    ));
}

#[test]
fn test_eat_some_food() {
    let (mut svm, program_id, payer) = setup();
    assert!(send_carnival(
        &mut svm,
        program_id,
        &payer,
        carnival_ix_data("Mary", 52, 3, "food", "Taco Shack")
    ));
}

#[test]
fn test_unknown_attraction_name_fails() {
    let (mut svm, program_id, payer) = setup();
    // "ride" is a valid attraction type, but there is no ride by this name, so the
    // program falls through its lookup table and returns InvalidInstructionData.
    assert!(!send_carnival(
        &mut svm,
        program_id,
        &payer,
        carnival_ix_data("Jimmy", 40, 5, "ride", "Roller Coaster")
    ));
}

#[test]
fn test_unknown_attraction_type_fails() {
    let (mut svm, program_id, payer) = setup();
    assert!(!send_carnival(
        &mut svm,
        program_id,
        &payer,
        carnival_ix_data("Jimmy", 40, 5, "spaceship", "Apollo")
    ));
}
