use {
    crate::{
        cpi::ReadPriceInstruction,
        instructions::{PythExampleError, MAXIMUM_PRICE_AGE_SECONDS, PYTH_RECEIVER_PROGRAM_ID},
    },
    quasar_test::prelude::*,
};

// Deterministic addresses keep tests independent of discovery order.
const PRICE_UPDATE: Pubkey = Pubkey::new_from_array([1; 32]);
const WRONG_OWNER: Pubkey = Pubkey::new_from_array([2; 32]);

/// The `publish_time` baked into the mock price update below.
const MOCK_PUBLISH_TIME: i64 = 1_700_000_000;

/// Build a minimal mock PriceUpdateV2 account body (133 bytes).
///
/// Layout:
///   [0..8]    Anchor discriminator for PriceUpdateV2
///   [8..40]   write_authority (zeroed)
///   [40]      verification_level = 1 (Full)
///   [41..73]  feed_id (0xEF * 32)
///   [73..81]  price = 15_000_000_000 i64 LE  (150.00 USD @ exponent -8)
///   [81..89]  conf = 100_000 u64 LE
///   [89..93]  exponent = -8 i32 LE
///   [93..101] publish_time = MOCK_PUBLISH_TIME i64 LE
///   [101..109] prev_publish_time = MOCK_PUBLISH_TIME - 1 i64 LE
///   [109..117] ema_price = 14_900_000_000 i64 LE
///   [117..125] ema_conf = 120_000 u64 LE
///   [125..133] posted_slot = 42 u64 LE
fn build_mock_price_update_data() -> Vec<u8> {
    let discriminator: [u8; 8] = [34, 241, 35, 99, 157, 126, 244, 205];
    let mut data = Vec::with_capacity(133);

    data.extend_from_slice(&discriminator);
    data.extend_from_slice(&[0u8; 32]); // write_authority
    data.push(1u8); // verification_level: Full
    data.extend_from_slice(&[0xEFu8; 32]); // feed_id
    data.extend_from_slice(&15_000_000_000i64.to_le_bytes()); // price
    data.extend_from_slice(&100_000u64.to_le_bytes()); // conf
    data.extend_from_slice(&(-8i32).to_le_bytes()); // exponent
    data.extend_from_slice(&MOCK_PUBLISH_TIME.to_le_bytes()); // publish_time
    data.extend_from_slice(&(MOCK_PUBLISH_TIME - 1).to_le_bytes()); // prev_publish_time
    data.extend_from_slice(&14_900_000_000i64.to_le_bytes()); // ema_price
    data.extend_from_slice(&120_000u64.to_le_bytes()); // ema_conf
    data.extend_from_slice(&42u64.to_le_bytes()); // posted_slot

    data
}

/// Install a hand-built Pyth oracle account (owner + raw data) into the world.
fn add_price_update_account(test: &mut Test, owner: Pubkey) {
    test.set_account(Account::new(
        PRICE_UPDATE,
        owner,
        1_000_000_000,
        build_mock_price_update_data(),
    ));
}

#[quasar_test]
fn read_price_accepts_a_fresh_price(test: &mut Test) {
    // A price exactly at the maximum allowed age is still accepted.
    test.warp_to_timestamp(MOCK_PUBLISH_TIME + MAXIMUM_PRICE_AGE_SECONDS);
    add_price_update_account(test, Pubkey::from(PYTH_RECEIVER_PROGRAM_ID));

    test.send(ReadPriceInstruction {
        price_update: PRICE_UPDATE,
    })
    .succeeds();
}

#[quasar_test]
fn read_price_rejects_a_stale_price(test: &mut Test) {
    // One second past the maximum age: rejected as stale.
    test.warp_to_timestamp(MOCK_PUBLISH_TIME + MAXIMUM_PRICE_AGE_SECONDS + 1);
    add_price_update_account(test, Pubkey::from(PYTH_RECEIVER_PROGRAM_ID));

    test.send(ReadPriceInstruction {
        price_update: PRICE_UPDATE,
    })
    .fails_with(PythExampleError::PriceTooOld);
}

#[quasar_test]
fn read_price_rejects_an_account_with_the_wrong_owner(test: &mut Test) {
    test.warp_to_timestamp(MOCK_PUBLISH_TIME);

    // Plausible price bytes, but the account is owned by some random program
    // instead of the Pyth Receiver: the owner constraint must reject it.
    add_price_update_account(test, WRONG_OWNER);

    test.send(ReadPriceInstruction {
        price_update: PRICE_UPDATE,
    })
    .fails_with(PythExampleError::PriceUpdateNotOwnedByPythReceiver);
}
