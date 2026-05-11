use {
    anchor_lang::{solana_program::instruction::Instruction, InstructionData, ToAccountMetas},
    litesvm::LiteSVM,
    solana_kite::{create_wallet, send_transaction_from_instructions},
    solana_signer::Signer,
};

fn setup() -> (LiteSVM, solana_keypair::Keypair) {
    let program_id = carnival::id();
    let mut svm = LiteSVM::new();
    let bytes = include_bytes!("../../../target/deploy/carnival.so");
    svm.add_program(program_id, bytes).unwrap();
    let payer = create_wallet(&mut svm, 10_000_000_000).unwrap();
    (svm, payer)
}

fn go_on_ride_ix(
    payer: &solana_keypair::Keypair,
    name: &str,
    height: u32,
    ticket_count: u32,
    ride_name: &str,
) -> Instruction {
    let accounts = carnival::accounts::CarnivalContext {
        payer: payer.pubkey(),
    }
    .to_account_metas(None);
    Instruction::new_with_bytes(
        carnival::id(),
        &carnival::instruction::GoOnRide {
            name: name.to_string(),
            height,
            ticket_count,
            ride_name: ride_name.to_string(),
        }
        .data(),
        accounts,
    )
}

fn play_game_ix(
    payer: &solana_keypair::Keypair,
    name: &str,
    ticket_count: u32,
    game_name: &str,
) -> Instruction {
    let accounts = carnival::accounts::CarnivalContext {
        payer: payer.pubkey(),
    }
    .to_account_metas(None);
    Instruction::new_with_bytes(
        carnival::id(),
        &carnival::instruction::PlayGame {
            name: name.to_string(),
            ticket_count,
            game_name: game_name.to_string(),
        }
        .data(),
        accounts,
    )
}

fn eat_food_ix(
    payer: &solana_keypair::Keypair,
    name: &str,
    ticket_count: u32,
    food_stand_name: &str,
) -> Instruction {
    let accounts = carnival::accounts::CarnivalContext {
        payer: payer.pubkey(),
    }
    .to_account_metas(None);
    Instruction::new_with_bytes(
        carnival::id(),
        &carnival::instruction::EatFood {
            name: name.to_string(),
            ticket_count,
            food_stand_name: food_stand_name.to_string(),
        }
        .data(),
        accounts,
    )
}

// Riders that have enough tickets and meet the height requirement succeed.
#[test]
fn test_go_on_ride_succeeds() {
    let (mut svm, payer) = setup();
    // Scrambler costs 3 tickets and requires 48" min height; Alice has 15
    // tickets and is 56" tall.
    let ix = go_on_ride_ix(&payer, "Alice", 56, 15, "Scrambler");
    send_transaction_from_instructions(&mut svm, vec![ix], &[&payer], &payer.pubkey()).unwrap();
}

// Insufficient tickets must produce a real error (NotEnoughTickets), not a
// silent Ok().
#[test]
fn test_go_on_ride_rejects_insufficient_tickets() {
    let (mut svm, payer) = setup();
    // Tilt-a-Whirl costs 3 tickets; Bob only has 1.
    let ix = go_on_ride_ix(&payer, "Bob", 49, 1, "Tilt-a-Whirl");
    let result =
        send_transaction_from_instructions(&mut svm, vec![ix], &[&payer], &payer.pubkey());
    assert!(result.is_err(), "ride must reject insufficient tickets");
}

// A rider below the minimum height must be rejected with RiderTooShort.
#[test]
fn test_go_on_ride_rejects_too_short() {
    let (mut svm, payer) = setup();
    // Ferris Wheel requires 55" min height; Jimmy is 36" tall but has plenty
    // of tickets.
    let ix = go_on_ride_ix(&payer, "Jimmy", 36, 15, "Ferris Wheel");
    let result =
        send_transaction_from_instructions(&mut svm, vec![ix], &[&payer], &payer.pubkey());
    assert!(result.is_err(), "ride must reject riders below min height");
}

// Unknown attraction name routes to ProgramError::InvalidInstructionData.
#[test]
fn test_go_on_ride_rejects_unknown_ride() {
    let (mut svm, payer) = setup();
    let ix = go_on_ride_ix(&payer, "Alice", 60, 100, "Quantum Coaster");
    let result =
        send_transaction_from_instructions(&mut svm, vec![ix], &[&payer], &payer.pubkey());
    assert!(result.is_err(), "unknown ride must be rejected");
}

#[test]
fn test_play_game_succeeds() {
    let (mut svm, payer) = setup();
    // Ring Toss costs 3 tickets; Alice has 15.
    let ix = play_game_ix(&payer, "Alice", 15, "Ring Toss");
    send_transaction_from_instructions(&mut svm, vec![ix], &[&payer], &payer.pubkey()).unwrap();
}

#[test]
fn test_play_game_rejects_insufficient_tickets() {
    let (mut svm, payer) = setup();
    // Ring Toss costs 3 tickets; Mary only has 1.
    let ix = play_game_ix(&payer, "Mary", 1, "Ring Toss");
    let result =
        send_transaction_from_instructions(&mut svm, vec![ix], &[&payer], &payer.pubkey());
    assert!(result.is_err(), "game must reject insufficient tickets");
}

#[test]
fn test_eat_food_succeeds() {
    let (mut svm, payer) = setup();
    // Dough Boy's costs 1 ticket; Bob has 6.
    let ix = eat_food_ix(&payer, "Bob", 6, "Dough Boy's");
    send_transaction_from_instructions(&mut svm, vec![ix], &[&payer], &payer.pubkey()).unwrap();
}

#[test]
fn test_eat_food_rejects_insufficient_tickets() {
    let (mut svm, payer) = setup();
    // Larry's Pizza costs 3 tickets; Mary has 1.
    let ix = eat_food_ix(&payer, "Mary", 1, "Larry's Pizza");
    let result =
        send_transaction_from_instructions(&mut svm, vec![ix], &[&payer], &payer.pubkey());
    assert!(result.is_err(), "food stand must reject insufficient tickets");
}
