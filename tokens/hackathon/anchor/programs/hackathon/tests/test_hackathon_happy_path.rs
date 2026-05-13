// Happy-path end-to-end test: a Squads 2-of-3 committee
// (Alice / Bob / Carol) creates a hackathon, registers a prize, funds the
// prize vault, sets a winner via multisig vote, then any wallet pays the
// winner. Verifies token balances at the end.

mod common;

use common::world::*;
use solana_kite::{get_token_account_balance, send_transaction_from_instructions};
use solana_signer::Signer;

#[test]
fn happy_path_create_fund_set_winner_pay() {
    let mut world = setup_world();

    // 1. create_hackathon (multisig-signed)
    let hackathon_name = "Anchor Bash 2026".to_string();
    let (hackathon_account, _bump) =
        hackathon_pda(&world.committee.vault, &hackathon_name);

    let create_ix = create_hackathon_instruction(
        &world.committee.vault,
        &world.committee.vault,
        &hackathon_account,
        hackathon_name.clone(),
    );
    run_through_multisig(&mut world, create_ix);

    // 2. add_prize (multisig-signed)
    let (prize_account, _bump) = prize_pda(&hackathon_account, 0);
    let vault_ata = prize_vault_address(&prize_account, &world.mint);
    let prize_amount = 100 * ONE_TOKEN;

    let add_ix = add_prize_instruction(
        &world.committee.vault,
        &world.committee.vault,
        &hackathon_account,
        &world.mint,
        &prize_account,
        &vault_ata,
        prize_amount,
    );
    run_through_multisig(&mut world, add_ix);

    // 3. Fund the prize vault. This is just an SPL mint_to from the test's
    //    mint authority — in production the committee would fund the vault
    //    via a Squads-signed token transfer.
    fund_prize_vault(&mut world, &vault_ata, prize_amount);

    // 4. set_winner (multisig-signed)
    let winner_wallet = solana_keypair::Keypair::new();
    world.svm.airdrop(&winner_wallet.pubkey(), 1_000_000_000).unwrap();
    let winner_ata = create_token_account_for(&mut world, &winner_wallet.pubkey());

    let set_winner_ix = set_winner_instruction(
        &world.committee.vault,
        &hackathon_account,
        &prize_account,
        0,
        winner_wallet.pubkey(),
    );
    run_through_multisig(&mut world, set_winner_ix);

    // 5. pay_winner — unpermissioned. Anyone can sign. We use a totally
    //    unrelated keypair to prove the call is not multisig-gated.
    let bystander = solana_kite::create_wallet(&mut world.svm, 1_000_000_000).unwrap();
    let pay_ix = pay_winner_instruction(
        &bystander.pubkey(),
        &hackathon_account,
        &prize_account,
        &world.mint,
        &vault_ata,
        &winner_ata,
        0,
    );
    send_transaction_from_instructions(
        &mut world.svm,
        vec![pay_ix],
        &[&bystander],
        &bystander.pubkey(),
    )
    .expect("bystander pays winner");

    // 6. Winner now holds exactly `prize_amount`.
    let balance = get_token_account_balance(&world.svm, &winner_ata).unwrap();
    assert_eq!(balance, prize_amount);
}


