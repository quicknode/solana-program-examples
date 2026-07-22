use {
    crate::cpi::{InitializeInstruction, UpdateInstruction},
    quasar_test::prelude::*,
};

// Deterministic addresses keep tests independent of discovery order.
const PAYER: Pubkey = Pubkey::new_from_array([1; 32]);
// The message account is a random keypair (not a PDA) - same as the Anchor
// version - so the caller passes its address explicitly.
const MESSAGE_ACCOUNT: Pubkey = Pubkey::new_from_array([2; 32]);

/// Assert the account holds the expected message in Quasar's compact wire
/// layout: disc(1 byte = 1) + u16 LE length prefix + message bytes.
/// `String<1024, 2>` uses a 2-byte length prefix: zeropod 0.3.3 rejects a
/// capacity that exceeds the prefix's range at compile time, so a 1024-byte
/// string can no longer use the default u8 prefix. Byte layout is part of
/// what this example demonstrates, so it is checked directly.
fn assert_message(test: &Test, expected: &str) {
    let account = test.account(MESSAGE_ACCOUNT).unwrap();
    assert_eq!(account.data[0], 1, "discriminator");
    let msg_len = u16::from_le_bytes(account.data[1..3].try_into().unwrap()) as usize;
    assert_eq!(msg_len, expected.len());
    assert_eq!(&account.data[3..3 + msg_len], expected.as_bytes());
}

#[quasar_test]
fn initialize_stores_the_message(test: &mut Test) {
    test.add(Wallet::new().at(PAYER));

    test.send(InitializeInstruction {
        payer: PAYER,
        message_account: MESSAGE_ACCOUNT,
        message: "Hello, World!".to_string().into(),
    })
    .succeeds();

    assert_message(test, "Hello, World!");
}

#[quasar_test]
fn update_with_a_longer_message_reallocs(test: &mut Test) {
    test.add(Wallet::new().at(PAYER));

    // Initialize with a short message.
    test.send(InitializeInstruction {
        payer: PAYER,
        message_account: MESSAGE_ACCOUNT,
        message: "Hi".to_string().into(),
    })
    .succeeds();

    // Update with a longer message - set_inner grows the account (realloc).
    let longer = "Hello, this is a much longer message!";
    test.send(UpdateInstruction {
        payer: PAYER,
        message_account: MESSAGE_ACCOUNT,
        message: longer.to_string().into(),
    })
    .succeeds();

    assert_message(test, longer);
}
