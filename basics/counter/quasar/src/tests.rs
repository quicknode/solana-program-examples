use {
    crate::{
        cpi::{IncrementInstruction, InitializeCounterInstruction},
        state::Counter,
    },
    quasar_test::prelude::*,
};

// Deterministic addresses keep tests independent of discovery order.
const PAYER: Pubkey = Pubkey::new_from_array([1; 32]);

#[quasar_test]
fn initialize_counter_creates_the_pda(test: &mut Test) {
    test.add(Wallet::new().at(PAYER));
    let counter = test.derive_pda(Counter::seeds(&PAYER));

    // The counter PDA and system program are canonical derivations, so the
    // generated instruction only asks for the payer.
    test.send(InitializeCounterInstruction { payer: PAYER })
        .succeeds();

    let state = test.read::<Counter>(counter);
    assert_eq!(u64::from(state.count), 0);
}

#[quasar_test]
fn increment_advances_the_count(test: &mut Test) {
    test.add(Wallet::new().at(PAYER));
    let counter = test.derive_pda(Counter::seeds(&PAYER));
    test.send(InitializeCounterInstruction { payer: PAYER })
        .succeeds();

    test.send(IncrementInstruction { counter }).succeeds();

    let state = test.read::<Counter>(counter);
    assert_eq!(u64::from(state.count), 1);
}
