// Lifecycle tests: cancel_prize (multisig refunds vault) and close_hackathon
// (only after every prize is paid or cancelled).

mod common;

use common::world::*;
use solana_kite::{get_token_account_balance, send_transaction_from_instructions};
use solana_signer::Signer;

const HACKATHON_NAME: &str = "Anchor Bash 2026";
const PRIZE_AMOUNT: u64 = 100 * ONE_TOKEN;

#[test]
fn cancel_prize_refunds_vault_to_committee_account() {
    let mut world = setup_world();
    // Bind keys to locals up front so we can pass them by value while also
    // holding a mutable borrow on `world`.
    let vault_key = world.committee.vault;
    let mint = world.mint;

    let (hackathon, _) = hackathon_pda(&vault_key, HACKATHON_NAME);
    let ix = create_hackathon_instruction(
        &vault_key,
        &vault_key,
        &hackathon,
        HACKATHON_NAME.to_string(),
    );
    run_through_multisig(&mut world, ix);

    let (prize, _) = prize_pda(&hackathon, 0);
    let prize_vault = prize_vault_address(&prize, &mint);
    let ix = add_prize_instruction(
        &vault_key,
        &vault_key,
        &hackathon,
        &mint,
        &prize,
        &prize_vault,
        PRIZE_AMOUNT,
    );
    run_through_multisig(&mut world, ix);

    fund_prize_vault(&mut world, &prize_vault, PRIZE_AMOUNT);
    let refund_target = create_token_account_for(&mut world, &vault_key);

    let ix = cancel_prize_instruction(
        &vault_key,
        &vault_key, // rent destination
        &hackathon,
        &prize,
        &mint,
        &prize_vault,
        &refund_target,
        0,
    );
    run_through_multisig(&mut world, ix);

    // Refund target now holds the full prize amount.
    assert_eq!(
        get_token_account_balance(&world.svm, &refund_target).unwrap(),
        PRIZE_AMOUNT
    );

    // Vault account is closed.
    assert!(
        world
            .svm
            .get_account(&prize_vault)
            .map(|a| a.data.is_empty())
            .unwrap_or(true),
        "vault should be closed after cancel_prize"
    );

    // A subsequent pay_winner attempt must fail: the prize is now cancelled.
    let winner = solana_keypair::Keypair::new();
    world
        .svm
        .airdrop(&winner.pubkey(), 1_000_000_000)
        .unwrap();
    let winner_ata = create_token_account_for(&mut world, &winner.pubkey());
    let attacker = solana_kite::create_wallet(&mut world.svm, 1_000_000_000).unwrap();
    let pay_ix = pay_winner_instruction(
        &attacker.pubkey(),
        &hackathon,
        &prize,
        &mint,
        &prize_vault,
        &winner_ata,
        0,
    );
    let result = send_transaction_from_instructions(
        &mut world.svm,
        vec![pay_ix],
        &[&attacker],
        &attacker.pubkey(),
    );
    assert!(
        result.is_err(),
        "pay_winner on cancelled prize must fail (vault closed + Cancelled flag)"
    );
}

#[test]
fn close_hackathon_succeeds_once_all_prizes_resolved() {
    let mut world = setup_world();
    let vault_key = world.committee.vault;
    let mint = world.mint;

    let (hackathon, _) = hackathon_pda(&vault_key, HACKATHON_NAME);
    let ix = create_hackathon_instruction(
        &vault_key,
        &vault_key,
        &hackathon,
        HACKATHON_NAME.to_string(),
    );
    run_through_multisig(&mut world, ix);

    // Add two prizes. We'll pay one and cancel the other.
    let mut prize_pdas = Vec::new();
    let mut prize_vaults = Vec::new();
    for index in 0..2u8 {
        let (prize, _) = prize_pda(&hackathon, index);
        let vault = prize_vault_address(&prize, &mint);
        let ix = add_prize_instruction(
            &vault_key,
            &vault_key,
            &hackathon,
            &mint,
            &prize,
            &vault,
            PRIZE_AMOUNT,
        );
        run_through_multisig(&mut world, ix);
        prize_pdas.push(prize);
        prize_vaults.push(vault);
    }

    // Pay prize 0.
    fund_prize_vault(&mut world, &prize_vaults[0], PRIZE_AMOUNT);
    let winner = solana_keypair::Keypair::new();
    world.svm.airdrop(&winner.pubkey(), 1_000_000_000).unwrap();
    let winner_ata = create_token_account_for(&mut world, &winner.pubkey());
    let ix = set_winner_instruction(
        &vault_key,
        &hackathon,
        &prize_pdas[0],
        0,
        winner.pubkey(),
    );
    run_through_multisig(&mut world, ix);
    let caller = solana_kite::create_wallet(&mut world.svm, 1_000_000_000).unwrap();
    let pay_ix = pay_winner_instruction(
        &caller.pubkey(),
        &hackathon,
        &prize_pdas[0],
        &mint,
        &prize_vaults[0],
        &winner_ata,
        0,
    );
    send_transaction_from_instructions(
        &mut world.svm,
        vec![pay_ix],
        &[&caller],
        &caller.pubkey(),
    )
    .expect("pay prize 0");

    // Cancel prize 1.
    let refund_target = create_token_account_for(&mut world, &vault_key);
    let ix = cancel_prize_instruction(
        &vault_key,
        &vault_key,
        &hackathon,
        &prize_pdas[1],
        &mint,
        &prize_vaults[1],
        &refund_target,
        1,
    );
    run_through_multisig(&mut world, ix);

    // Now close_hackathon should succeed.
    let close_ix = close_hackathon_instruction(&vault_key, &vault_key, &hackathon, &prize_pdas);
    run_through_multisig(&mut world, close_ix);

    // Hackathon account is closed (zero-length data or absent).
    let closed = world.svm.get_account(&hackathon);
    assert!(
        closed.map(|a| a.data.is_empty()).unwrap_or(true),
        "hackathon account should be closed"
    );
}

#[test]
fn close_hackathon_fails_while_prize_is_still_active() {
    let mut world = setup_world();
    let vault_key = world.committee.vault;
    let mint = world.mint;

    let (hackathon, _) = hackathon_pda(&vault_key, HACKATHON_NAME);
    let ix = create_hackathon_instruction(
        &vault_key,
        &vault_key,
        &hackathon,
        HACKATHON_NAME.to_string(),
    );
    run_through_multisig(&mut world, ix);

    let (prize, _) = prize_pda(&hackathon, 0);
    let prize_vault = prize_vault_address(&prize, &mint);
    let ix = add_prize_instruction(
        &vault_key,
        &vault_key,
        &hackathon,
        &mint,
        &prize,
        &prize_vault,
        PRIZE_AMOUNT,
    );
    run_through_multisig(&mut world, ix);

    // Try to close while the prize is neither paid nor cancelled. The
    // `vault_transaction_execute` instruction must fail because the inner
    // close_hackathon returns `PrizesStillActive`.
    let close_ix = close_hackathon_instruction(&vault_key, &vault_key, &hackathon, &[prize]);
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        run_through_multisig(&mut world, close_ix);
    }));
    assert!(
        result.is_err(),
        "close_hackathon must fail while prizes remain active"
    );
}
