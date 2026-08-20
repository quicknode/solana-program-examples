use {crate::cpi::GoToParkInstruction, quasar_test::prelude::*};

// Deterministic addresses keep tests independent of discovery order.
const USER: Pubkey = Pubkey::new_from_array([1; 32]);

#[quasar_test]
fn tall_visitor_is_allowed_on_the_ride(test: &mut Test) {
    test.add(Wallet::new().at(USER));

    let outcome = test.send(GoToParkInstruction {
        signer: USER,
        height: 6,
        name: "Alice".to_string().into(),
    });
    outcome.succeeds();

    let logs = outcome.logs().join("\n");
    assert!(logs.contains("Welcome to the park!"), "should welcome");
    assert!(
        logs.contains("tall enough to ride"),
        "should say tall enough"
    );
}

#[quasar_test]
fn short_visitor_is_turned_away(test: &mut Test) {
    test.add(Wallet::new().at(USER));

    let outcome = test.send(GoToParkInstruction {
        signer: USER,
        height: 3,
        name: "Bob".to_string().into(),
    });
    outcome.succeeds();

    let logs = outcome.logs().join("\n");
    assert!(logs.contains("Welcome to the park!"), "should welcome");
    assert!(
        logs.contains("NOT tall enough"),
        "should say not tall enough"
    );
}
