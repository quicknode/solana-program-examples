use {
    crate::{
        cpi::{CreatePageVisitsInstruction, IncrementPageVisitsInstruction},
        state::PageVisits,
    },
    quasar_test::prelude::*,
};

// Deterministic addresses keep tests independent of discovery order.
const PAYER: Pubkey = Pubkey::new_from_array([1; 32]);

#[quasar_test]
fn create_page_visits_initializes_the_pda(test: &mut Test) {
    test.add(Wallet::new().at(PAYER));
    let page_visits = test.derive_pda(PageVisits::seeds(&PAYER));

    // The page-visits PDA and system program are canonical derivations, so
    // the generated instruction only asks for the payer.
    test.send(CreatePageVisitsInstruction { payer: PAYER }).succeeds();

    // Byte layout is part of what this example demonstrates:
    // 1 byte discriminator (1) + 8 bytes u64 count (0).
    let account = test.account(page_visits).unwrap();
    assert_eq!(account.data.len(), 9);
    assert_eq!(account.data[0], 1); // discriminator
    assert_eq!(&account.data[1..], &[0u8; 8]); // page_visits = 0

    let state = test.read::<PageVisits>(page_visits);
    assert_eq!(u64::from(state.page_visits), 0);
}

#[quasar_test]
fn increment_page_visits_advances_the_count(test: &mut Test) {
    test.add(Wallet::new().at(PAYER));
    let page_visits = test.derive_pda(PageVisits::seeds(&PAYER));
    test.send(CreatePageVisitsInstruction { payer: PAYER }).succeeds();

    // The user account is only used for PDA derivation, not as a signer.
    test.send(IncrementPageVisitsInstruction {
        user: PAYER,
        page_visits,
    })
    .succeeds();

    let state = test.read::<PageVisits>(page_visits);
    assert_eq!(
        u64::from(state.page_visits),
        1,
        "page_visits should be 1 after one increment"
    );
}
