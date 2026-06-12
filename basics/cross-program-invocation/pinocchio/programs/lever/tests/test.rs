use litesvm::LiteSVM;
use solana_instruction::{AccountMeta, Instruction};
use solana_keypair::{Keypair, Signer};
use solana_native_token::LAMPORTS_PER_SOL;
use solana_pubkey::Pubkey;
use solana_transaction::Transaction;

// Both .so files are built into ../../../tests/fixtures (the example root) by
// `pnpm build` / `pnpm build-and-test`, which run `cargo build-sbf` for each
// program with --sbf-out-dir set there. Run that before `cargo test`.
const HAND_SO: &[u8] =
    include_bytes!("../../../tests/fixtures/cross_program_invocation_pinocchio_hand.so");
const LEVER_SO: &[u8] =
    include_bytes!("../../../tests/fixtures/cross_program_invocation_pinocchio_lever.so");

// Lever instruction discriminators.
const IX_INITIALIZE: u8 = 0;

fn setup() -> (LiteSVM, Pubkey, Pubkey, Keypair) {
    let hand_id = Pubkey::new_unique();
    let lever_id = Pubkey::new_unique();

    let mut svm = LiteSVM::new();
    svm.add_program(hand_id, HAND_SO).unwrap();
    svm.add_program(lever_id, LEVER_SO).unwrap();

    let payer = Keypair::new();
    svm.airdrop(&payer.pubkey(), LAMPORTS_PER_SOL * 10).unwrap();

    (svm, hand_id, lever_id, payer)
}

// Calls lever's `initialize`, which creates the single-byte power account under
// the lever program via a CPI to the System Program.
fn initialize(svm: &mut LiteSVM, lever_id: Pubkey, power: &Keypair, payer: &Keypair) {
    let ix = Instruction {
        program_id: lever_id,
        accounts: vec![
            AccountMeta::new(power.pubkey(), true),
            AccountMeta::new(payer.pubkey(), true),
            AccountMeta::new_readonly(solana_system_interface::program::ID, false),
        ],
        data: vec![IX_INITIALIZE],
    };
    let tx = Transaction::new_signed_with_payer(
        &[ix],
        Some(&payer.pubkey()),
        &[payer, power],
        svm.latest_blockhash(),
    );
    svm.send_transaction(tx).unwrap();
}

// Calls hand, which forwards `switch_power(name)` to lever over a CPI.
fn pull_lever(
    svm: &mut LiteSVM,
    hand_id: Pubkey,
    lever_id: Pubkey,
    power: Pubkey,
    payer: &Keypair,
    name: &str,
) {
    let ix = Instruction {
        program_id: hand_id,
        accounts: vec![
            AccountMeta::new(power, false),
            AccountMeta::new_readonly(lever_id, false),
        ],
        data: name.as_bytes().to_vec(),
    };
    let tx = Transaction::new_signed_with_payer(
        &[ix],
        Some(&payer.pubkey()),
        &[payer],
        svm.latest_blockhash(),
    );
    svm.send_transaction(tx).unwrap();
}

fn power_byte(svm: &LiteSVM, power: Pubkey) -> u8 {
    svm.get_account(&power).unwrap().data[0]
}

#[test]
fn test_cpi_toggles_power() {
    let (mut svm, hand_id, lever_id, payer) = setup();
    let power = Keypair::new();

    initialize(&mut svm, lever_id, &power, &payer);
    assert_eq!(power_byte(&svm, power.pubkey()), 0, "power starts off");

    pull_lever(&mut svm, hand_id, lever_id, power.pubkey(), &payer, "Chris");
    assert_eq!(
        power_byte(&svm, power.pubkey()),
        1,
        "power on after first pull"
    );

    pull_lever(
        &mut svm,
        hand_id,
        lever_id,
        power.pubkey(),
        &payer,
        "Ashley",
    );
    assert_eq!(
        power_byte(&svm, power.pubkey()),
        0,
        "power off after second pull"
    );
}

#[test]
fn test_lever_rejects_unknown_discriminator() {
    let (mut svm, _hand_id, lever_id, payer) = setup();
    let power = Keypair::new();

    initialize(&mut svm, lever_id, &power, &payer);

    let ix = Instruction {
        program_id: lever_id,
        accounts: vec![AccountMeta::new(power.pubkey(), false)],
        data: vec![42],
    };
    let tx = Transaction::new_signed_with_payer(
        &[ix],
        Some(&payer.pubkey()),
        &[&payer],
        svm.latest_blockhash(),
    );
    assert!(
        svm.send_transaction(tx).is_err(),
        "unknown discriminator must fail"
    );
}
