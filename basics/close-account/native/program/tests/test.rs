use close_account_native_program::state::user::User;
use litesvm::LiteSVM;
use solana_instruction::{AccountMeta, Instruction};
use solana_keypair::{Keypair, Signer};
use solana_native_token::LAMPORTS_PER_SOL;
use solana_pubkey::Pubkey;
use solana_transaction::Transaction;

use close_account_native_program::processor::MyInstruction;

/// LiteSVM's default fee: 5000 lamports per signature, one signer per
/// transaction in these tests.
const TRANSACTION_FEE_LAMPORTS: u64 = 5000;

fn setup() -> (LiteSVM, Pubkey) {
    let mut svm = LiteSVM::new();
    let program_id = Pubkey::new_unique();
    let program_bytes = include_bytes!("../../tests/fixtures/close_account_native_program.so");
    svm.add_program(program_id, program_bytes).unwrap();
    (svm, program_id)
}

fn funded_keypair(svm: &mut LiteSVM) -> Keypair {
    let keypair = Keypair::new();
    svm.airdrop(&keypair.pubkey(), LAMPORTS_PER_SOL * 10)
        .unwrap();
    keypair
}

fn user_pda(program_id: &Pubkey, user: &Pubkey) -> Pubkey {
    Pubkey::find_program_address(&[User::SEED_PREFIX.as_bytes(), user.as_ref()], program_id).0
}

fn create_user_instruction(program_id: Pubkey, target: Pubkey, payer: Pubkey) -> Instruction {
    let data = borsh::to_vec(&MyInstruction::CreateUser(User {
        name: "Jacob".to_string(),
    }))
    .unwrap();
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

fn close_user_instruction(
    program_id: Pubkey,
    target: Pubkey,
    payer: Pubkey,
    payer_is_signer: bool,
) -> Instruction {
    let data = borsh::to_vec(&MyInstruction::CloseUser).unwrap();
    Instruction {
        program_id,
        accounts: vec![
            AccountMeta::new(target, false),
            AccountMeta::new(payer, payer_is_signer),
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

#[test]
fn close_returns_all_lamports_to_owner() {
    let (mut svm, program_id) = setup();
    let payer = funded_keypair(&mut svm);
    let target = user_pda(&program_id, &payer.pubkey());

    send(
        &mut svm,
        create_user_instruction(program_id, target, payer.pubkey()),
        &payer,
    )
    .unwrap();

    let target_lamports = svm.get_account(&target).unwrap().lamports;
    assert!(target_lamports > 0, "created PDA should hold rent lamports");
    let payer_balance_before_close = svm.get_balance(&payer.pubkey()).unwrap();

    send(
        &mut svm,
        close_user_instruction(program_id, target, payer.pubkey(), true),
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
    let victim_account = user_pda(&program_id, &victim.pubkey());

    send(
        &mut svm,
        create_user_instruction(program_id, victim_account, victim.pubkey()),
        &victim,
    )
    .unwrap();

    // The attacker signs, but the target is the victim's PDA, not the
    // attacker's, so the seeds check fails.
    let result = send(
        &mut svm,
        close_user_instruction(program_id, victim_account, attacker.pubkey(), true),
        &attacker,
    );
    assert!(result.is_err(), "non-owner close must fail");

    // The victim's account is untouched.
    let victim_account_after = svm.get_account(&victim_account).unwrap();
    assert_eq!(victim_account_after.owner, program_id);
    assert!(victim_account_after.lamports > 0);
}

#[test]
fn close_rejects_payer_that_did_not_sign() {
    let (mut svm, program_id) = setup();
    let victim = funded_keypair(&mut svm);
    let attacker = funded_keypair(&mut svm);
    let victim_account = user_pda(&program_id, &victim.pubkey());

    send(
        &mut svm,
        create_user_instruction(program_id, victim_account, victim.pubkey()),
        &victim,
    )
    .unwrap();

    // The attacker names the victim as the payer without the victim's
    // signature: rejected by the signer check.
    let result = send(
        &mut svm,
        close_user_instruction(program_id, victim_account, victim.pubkey(), false),
        &attacker,
    );
    assert!(
        result.is_err(),
        "close without the owner's signature must fail"
    );

    let victim_account_after = svm.get_account(&victim_account).unwrap();
    assert_eq!(victim_account_after.owner, program_id);
    assert!(victim_account_after.lamports > 0);
}
