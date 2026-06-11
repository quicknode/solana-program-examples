use litesvm::LiteSVM;
use solana_instruction::{AccountMeta, Instruction};
use solana_keypair::{Keypair, Signer};
use solana_native_token::LAMPORTS_PER_SOL;
use solana_pubkey::Pubkey;
use solana_transaction::Transaction;

use close_account_pinocchio_program::{User, CLOSE_DISCRIMINATOR, CREATE_DISCRIMINATOR};

/// LiteSVM's default fee: 5000 lamports per signature, one signer per
/// transaction in these tests.
const TRANSACTION_FEE_LAMPORTS: u64 = 5000;

fn setup() -> (LiteSVM, Pubkey) {
    let mut svm = LiteSVM::new();
    let program_id = Pubkey::new_unique();
    let program_bytes = include_bytes!("../../tests/fixtures/close_account_pinocchio_program.so");
    svm.add_program(program_id, program_bytes).unwrap();
    (svm, program_id)
}

fn funded_keypair(svm: &mut LiteSVM) -> Keypair {
    let keypair = Keypair::new();
    svm.airdrop(&keypair.pubkey(), LAMPORTS_PER_SOL * 10)
        .unwrap();
    keypair
}

fn user_pda(program_id: &Pubkey, user: &Pubkey) -> (Pubkey, u8) {
    Pubkey::find_program_address(&[User::SEED_PREFIX.as_bytes(), user.as_ref()], program_id)
}

fn create_user_data(bump: u8) -> Vec<u8> {
    let mut data = Vec::new();
    data.push(CREATE_DISCRIMINATOR);
    data.push(bump);
    let mut name = [0u8; User::LEN];
    let name_len = b"Jacob".len().min(User::LEN);
    name[..name_len].copy_from_slice(&b"Jacob"[..name_len]);
    data.extend_from_slice(&name);
    data
}

fn user_instruction(
    program_id: Pubkey,
    target: Pubkey,
    payer: Pubkey,
    data: Vec<u8>,
) -> Instruction {
    Instruction {
        program_id,
        accounts: vec![
            AccountMeta::new(target, false),
            AccountMeta::new(payer, true),
            AccountMeta::new(solana_system_interface::program::ID, false),
        ],
        data,
    }
}

fn send(svm: &mut LiteSVM, instruction: Instruction, payer: &Keypair) -> Result<(), String> {
    let tx = Transaction::new_signed_with_payer(
        &[instruction],
        Some(&payer.pubkey()),
        &[payer],
        svm.latest_blockhash(),
    );
    svm.send_transaction(tx)
        .map(|_| ())
        .map_err(|failed| format!("{:?}", failed.err))
}

fn create_user_account(svm: &mut LiteSVM, program_id: Pubkey, payer: &Keypair) -> Pubkey {
    let (target, bump) = user_pda(&program_id, &payer.pubkey());
    send(
        svm,
        user_instruction(program_id, target, payer.pubkey(), create_user_data(bump)),
        payer,
    )
    .unwrap();
    target
}

#[test]
fn create_then_close_returns_all_lamports() {
    let (mut svm, program_id) = setup();
    let payer = funded_keypair(&mut svm);

    let target = create_user_account(&mut svm, program_id, &payer);

    let created = svm.get_account(&target).unwrap();
    assert_eq!(created.data.len(), User::LEN);
    assert_eq!(created.owner, program_id);
    assert_eq!(&created.data[..5], b"Jacob");
    let target_lamports = created.lamports;
    assert!(target_lamports > 0, "created PDA should hold rent lamports");

    let payer_balance_before_close = svm.get_balance(&payer.pubkey()).unwrap();

    send(
        &mut svm,
        user_instruction(
            program_id,
            target,
            payer.pubkey(),
            vec![CLOSE_DISCRIMINATOR],
        ),
        &payer,
    )
    .unwrap();

    // Every lamport in the PDA comes back to the payer; only the
    // transaction fee is lost.
    let payer_balance_after_close = svm.get_balance(&payer.pubkey()).unwrap();
    assert_eq!(
        payer_balance_after_close,
        payer_balance_before_close + target_lamports - TRANSACTION_FEE_LAMPORTS,
    );

    // The drained account no longer exists (0 lamports, no data).
    let closed = svm.get_account(&target);
    assert!(
        closed.is_none() || closed.unwrap().lamports == 0,
        "closed account should hold no lamports",
    );
}

#[test]
fn close_rejects_non_owner() {
    let (mut svm, program_id) = setup();
    let victim = funded_keypair(&mut svm);
    let attacker = funded_keypair(&mut svm);

    let victim_account = create_user_account(&mut svm, program_id, &victim);

    // The attacker signs, but the target is the victim's PDA, not the
    // attacker's, so the seeds check fails.
    let result = send(
        &mut svm,
        user_instruction(
            program_id,
            victim_account,
            attacker.pubkey(),
            vec![CLOSE_DISCRIMINATOR],
        ),
        &attacker,
    );
    assert!(result.is_err(), "non-owner close must fail");

    let victim_account_after = svm.get_account(&victim_account).unwrap();
    assert_eq!(victim_account_after.owner, program_id);
    assert!(victim_account_after.lamports > 0);
}

#[test]
fn close_rejects_payer_that_did_not_sign() {
    let (mut svm, program_id) = setup();
    let victim = funded_keypair(&mut svm);
    let attacker = funded_keypair(&mut svm);

    let victim_account = create_user_account(&mut svm, program_id, &victim);

    // The attacker names the victim as the payer without the victim's
    // signature: rejected by the signer check.
    let close_with_unsigned_payer = Instruction {
        program_id,
        accounts: vec![
            AccountMeta::new(victim_account, false),
            AccountMeta::new(victim.pubkey(), false),
            AccountMeta::new(solana_system_interface::program::ID, false),
        ],
        data: vec![CLOSE_DISCRIMINATOR],
    };
    let result = send(&mut svm, close_with_unsigned_payer, &attacker);
    assert!(
        result.is_err(),
        "close without the owner's signature must fail"
    );

    let victim_account_after = svm.get_account(&victim_account).unwrap();
    assert_eq!(victim_account_after.owner, program_id);
    assert!(victim_account_after.lamports > 0);
}

#[test]
fn create_rejects_wrong_bump() {
    let (mut svm, program_id) = setup();
    let payer = funded_keypair(&mut svm);
    let (target, bump) = user_pda(&program_id, &payer.pubkey());

    let wrong_bump = bump.wrapping_sub(1);
    let result = send(
        &mut svm,
        user_instruction(
            program_id,
            target,
            payer.pubkey(),
            create_user_data(wrong_bump),
        ),
        &payer,
    );
    assert!(
        result.is_err(),
        "create with a non-canonical bump must fail"
    );
    assert!(svm.get_account(&target).is_none());
}

#[test]
fn create_rejects_short_instruction_data() {
    let (mut svm, program_id) = setup();
    let payer = funded_keypair(&mut svm);
    let (target, bump) = user_pda(&program_id, &payer.pubkey());

    // Discriminator plus bump only: name bytes are missing entirely.
    let result = send(
        &mut svm,
        user_instruction(
            program_id,
            target,
            payer.pubkey(),
            vec![CREATE_DISCRIMINATOR, bump],
        ),
        &payer,
    );
    assert!(result.is_err(), "create with short data must fail cleanly");
    assert!(svm.get_account(&target).is_none());
}
