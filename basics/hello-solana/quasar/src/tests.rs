use {crate::cpi::HelloInstruction, quasar_test::prelude::*};

// Deterministic addresses keep tests independent of discovery order.
const PAYER: Pubkey = Pubkey::new_from_array([1; 32]);

#[quasar_test]
fn hello_logs_the_greeting(test: &mut Test) {
    test.add(Wallet::new().at(PAYER));

    let outcome = test.send(HelloInstruction { payer: PAYER });
    outcome.succeeds();

    // The program only logs; assert it emitted its greeting, not just that
    // the transaction succeeded.
    let logs = outcome.logs().join("\n");
    assert!(
        logs.contains("Hello, Solana!"),
        "expected the program to log its greeting, got:\n{logs}"
    );
}
