use {
    crate::cpi::{EatFoodInstruction, GoOnRideInstruction, PlayGameInstruction},
    quasar_test::prelude::*,
};

// Deterministic addresses keep tests independent of discovery order.
const USER: Pubkey = Pubkey::new_from_array([1; 32]);

fn go_on_ride(name: &str, height: u32, ticket_count: u32, ride_name: &str) -> GoOnRideInstruction {
    GoOnRideInstruction {
        payer: USER,
        height,
        ticket_count,
        name: name.to_string().into(),
        ride_name: ride_name.to_string().into(),
    }
}

fn play_game(name: &str, ticket_count: u32, game_name: &str) -> PlayGameInstruction {
    PlayGameInstruction {
        payer: USER,
        ticket_count,
        name: name.to_string().into(),
        game_name: game_name.to_string().into(),
    }
}

fn eat_food(name: &str, ticket_count: u32, food_stand_name: &str) -> EatFoodInstruction {
    EatFoodInstruction {
        payer: USER,
        ticket_count,
        name: name.to_string().into(),
        food_stand_name: food_stand_name.to_string().into(),
    }
}

#[quasar_test]
fn tall_rider_with_tickets_boards_the_ride(test: &mut Test) {
    test.add(Wallet::new().at(USER));

    let outcome = test.send(go_on_ride("Alice", 60, 5, "Ferris Wheel"));
    outcome.succeeds();

    let logs = outcome.logs().join("\n");
    assert!(logs.contains("about to go on a ride"), "should announce ride");
    assert!(logs.contains("Welcome aboard"), "should welcome aboard");
}

#[quasar_test]
fn short_rider_is_turned_away(test: &mut Test) {
    test.add(Wallet::new().at(USER));

    let outcome = test.send(go_on_ride("Bob", 40, 5, "Ferris Wheel"));
    outcome.succeeds();

    let logs = outcome.logs().join("\n");
    assert!(logs.contains("not tall enough"), "should reject short rider");
}

#[quasar_test]
fn rider_without_enough_tickets_is_turned_away(test: &mut Test) {
    test.add(Wallet::new().at(USER));

    let outcome = test.send(go_on_ride("Charlie", 60, 1, "Zero Gravity"));
    outcome.succeeds();

    let logs = outcome.logs().join("\n");
    assert!(logs.contains("enough tickets"), "should reject insufficient tickets");
}

#[quasar_test]
fn upside_down_ride_warns_the_rider(test: &mut Test) {
    test.add(Wallet::new().at(USER));

    let outcome = test.send(go_on_ride("Dave", 65, 5, "Zero Gravity"));
    outcome.succeeds();

    let logs = outcome.logs().join("\n");
    assert!(logs.contains("upside down"), "should warn about upside down");
}

#[quasar_test]
fn player_with_tickets_plays_the_game(test: &mut Test) {
    test.add(Wallet::new().at(USER));

    let outcome = test.send(play_game("Alice", 5, "Ring Toss"));
    outcome.succeeds();

    let logs = outcome.logs().join("\n");
    assert!(logs.contains("about to play"), "should announce game");
    assert!(logs.contains("what you got"), "should encourage player");
}

#[quasar_test]
fn player_without_enough_tickets_is_turned_away(test: &mut Test) {
    test.add(Wallet::new().at(USER));

    let outcome = test.send(play_game("Bob", 1, "Ring Toss"));
    outcome.succeeds();

    let logs = outcome.logs().join("\n");
    assert!(logs.contains("enough tickets"), "should reject insufficient tickets");
}

#[quasar_test]
fn visitor_with_tickets_eats_at_the_stand(test: &mut Test) {
    test.add(Wallet::new().at(USER));

    let outcome = test.send(eat_food("Alice", 3, "Larry's Pizza"));
    outcome.succeeds();

    let logs = outcome.logs().join("\n");
    assert!(logs.contains("food stand"), "should welcome to food stand");
    assert!(logs.contains("Enjoy"), "should say enjoy");
}

#[quasar_test]
fn visitor_without_enough_tickets_cannot_eat(test: &mut Test) {
    test.add(Wallet::new().at(USER));

    let outcome = test.send(eat_food("Bob", 0, "Larry's Pizza"));
    outcome.succeeds();

    let logs = outcome.logs().join("\n");
    assert!(logs.contains("enough tickets"), "should reject insufficient tickets");
}

#[quasar_test]
fn unknown_ride_name_is_rejected(test: &mut Test) {
    test.add(Wallet::new().at(USER));

    test.send(go_on_ride("Eve", 60, 5, "Nonexistent Ride"))
        .fails(ProgramError::InvalidInstructionData);
}
