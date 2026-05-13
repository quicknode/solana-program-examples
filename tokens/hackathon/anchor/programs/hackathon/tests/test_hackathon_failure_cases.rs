// Failure cases: pay_winner before funded / before winner / after paid, and
// the program-level signer check refusing a non-multisig caller on a
// privileged handler.

mod common;

use common::world::*;
use solana_kite::send_transaction_from_instructions;
use solana_signer::Signer;

const HACKATHON_NAME: &str = "Anchor Bash 2026";
const PRIZE_AMOUNT: u64 = 100 * ONE_TOKEN;

// Bring a hackathon + one registered prize into existence, returning the
// hackathon PDA, prize PDA and vault ATA. Vault is NOT funded; no winner is
// set. Each failure-case test starts from this minimal state and only
// performs the steps it needs.
struct PrizeWorld {
    world: World,
    hackathon: anchor_lang::prelude::Pubkey,
    prize: anchor_lang::prelude::Pubkey,
    vault: anchor_lang::prelude::Pubkey,
}

fn setup_with_one_prize() -> PrizeWorld {
    let mut world = setup_world();
    let (hackathon, _) = hackathon_pda(&world.committee.vault, HACKATHON_NAME);
    let create_ix = create_hackathon_instruction(
        &world.committee.vault,
        &world.committee.vault,
        &hackathon,
        HACKATHON_NAME.to_string(),
    );
    run_through_multisig(&mut world, create_ix);

    let (prize, _) = prize_pda(&hackathon, 0);
    let vault = prize_vault_address(&prize, &world.mint);
    let add_ix = add_prize_instruction(
        &world.committee.vault,
        &world.committee.vault,
        &hackathon,
        &world.mint,
        &prize,
        &vault,
        PRIZE_AMOUNT,
    );
    run_through_multisig(&mut world, add_ix);

    PrizeWorld {
        world,
        hackathon,
        prize,
        vault,
    }
}

#[test]
fn pay_winner_fails_when_no_winner_set() {
    let mut pw = setup_with_one_prize();
    fund_prize_vault(&mut pw.world, &pw.vault, PRIZE_AMOUNT);

    let winner = solana_keypair::Keypair::new();
    pw.world
        .svm
        .airdrop(&winner.pubkey(), 1_000_000_000)
        .unwrap();
    let winner_ata = create_token_account_for(&mut pw.world, &winner.pubkey());

    let caller = solana_kite::create_wallet(&mut pw.world.svm, 1_000_000_000).unwrap();
    let ix = pay_winner_instruction(
        &caller.pubkey(),
        &pw.hackathon,
        &pw.prize,
        &pw.world.mint,
        &pw.vault,
        &winner_ata,
        0,
    );
    let result = send_transaction_from_instructions(
        &mut pw.world.svm,
        vec![ix],
        &[&caller],
        &caller.pubkey(),
    );
    assert!(result.is_err(), "expected NoWinner failure");
}

#[test]
fn pay_winner_fails_when_vault_underfunded() {
    let mut pw = setup_with_one_prize();
    // Fund the vault with less than the prize amount.
    fund_prize_vault(&mut pw.world, &pw.vault, PRIZE_AMOUNT - 1);

    let winner = solana_keypair::Keypair::new();
    pw.world
        .svm
        .airdrop(&winner.pubkey(), 1_000_000_000)
        .unwrap();
    let winner_ata = create_token_account_for(&mut pw.world, &winner.pubkey());

    let set_ix = set_winner_instruction(
        &pw.world.committee.vault,
        &pw.hackathon,
        &pw.prize,
        0,
        winner.pubkey(),
    );
    run_through_multisig(&mut pw.world, set_ix);

    let caller = solana_kite::create_wallet(&mut pw.world.svm, 1_000_000_000).unwrap();
    let ix = pay_winner_instruction(
        &caller.pubkey(),
        &pw.hackathon,
        &pw.prize,
        &pw.world.mint,
        &pw.vault,
        &winner_ata,
        0,
    );
    let result = send_transaction_from_instructions(
        &mut pw.world.svm,
        vec![ix],
        &[&caller],
        &caller.pubkey(),
    );
    assert!(result.is_err(), "expected Underfunded failure");
}

#[test]
fn pay_winner_fails_when_already_paid() {
    let mut pw = setup_with_one_prize();
    fund_prize_vault(&mut pw.world, &pw.vault, PRIZE_AMOUNT);

    let winner = solana_keypair::Keypair::new();
    pw.world
        .svm
        .airdrop(&winner.pubkey(), 1_000_000_000)
        .unwrap();
    let winner_ata = create_token_account_for(&mut pw.world, &winner.pubkey());

    let set_ix = set_winner_instruction(
        &pw.world.committee.vault,
        &pw.hackathon,
        &pw.prize,
        0,
        winner.pubkey(),
    );
    run_through_multisig(&mut pw.world, set_ix);

    // First payment succeeds.
    let caller = solana_kite::create_wallet(&mut pw.world.svm, 1_000_000_000).unwrap();
    let ix1 = pay_winner_instruction(
        &caller.pubkey(),
        &pw.hackathon,
        &pw.prize,
        &pw.world.mint,
        &pw.vault,
        &winner_ata,
        0,
    );
    send_transaction_from_instructions(
        &mut pw.world.svm,
        vec![ix1],
        &[&caller],
        &caller.pubkey(),
    )
    .expect("first pay_winner succeeds");

    // Second payment must fail. Re-fund the vault first so we're testing
    // the AlreadyPaid guard, not the Underfunded guard.
    fund_prize_vault(&mut pw.world, &pw.vault, PRIZE_AMOUNT);
    let ix2 = pay_winner_instruction(
        &caller.pubkey(),
        &pw.hackathon,
        &pw.prize,
        &pw.world.mint,
        &pw.vault,
        &winner_ata,
        0,
    );
    let result = send_transaction_from_instructions(
        &mut pw.world.svm,
        vec![ix2],
        &[&caller],
        &caller.pubkey(),
    );
    assert!(result.is_err(), "expected AlreadyPaid failure");
}

#[test]
fn set_winner_fails_when_signer_is_not_multisig_authority() {
    let mut pw = setup_with_one_prize();

    // An attacker signs `set_winner` directly with their own keypair instead
    // of going through the Squads vault. The program's `has_one = authority`
    // constraint on the Hackathon account must reject this.
    let attacker = solana_kite::create_wallet(&mut pw.world.svm, 1_000_000_000).unwrap();
    let winner = solana_keypair::Keypair::new();

    let ix = set_winner_instruction(
        &attacker.pubkey(),
        &pw.hackathon,
        &pw.prize,
        0,
        winner.pubkey(),
    );
    let result = send_transaction_from_instructions(
        &mut pw.world.svm,
        vec![ix],
        &[&attacker],
        &attacker.pubkey(),
    );
    assert!(
        result.is_err(),
        "non-multisig signer must not be accepted on set_winner"
    );
}
