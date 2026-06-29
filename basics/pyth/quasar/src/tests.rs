use quasar_svm::{Account, Instruction, Pubkey, QuasarSvm};
use solana_address::Address;

use crate::instructions::{
    PythExampleError, MAXIMUM_PRICE_AGE_SECONDS, PYTH_RECEIVER_PROGRAM_ID,
};

/// The `publish_time` baked into the mock price update below.
const MOCK_PUBLISH_TIME: i64 = 1_700_000_000;

fn setup() -> QuasarSvm {
    let elf = include_bytes!("../target/deploy/quasar_pyth_example.so");
    QuasarSvm::new().with_program(&Pubkey::from(crate::ID), elf)
}

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
fn build_mock_price_update_account() -> Vec<u8> {
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

fn price_update_account(address: Pubkey, owner: Pubkey) -> Account {
    Account {
        address,
        lamports: 1_000_000_000,
        data: build_mock_price_update_account(),
        owner,
        executable: false,
    }
}

fn read_price_instruction(price_update: Pubkey) -> Instruction {
    Instruction {
        program_id: Pubkey::from(crate::ID),
        accounts: vec![solana_instruction::AccountMeta::new_readonly(
            Address::from(price_update.to_bytes()),
            false,
        )],
        data: vec![0u8], // read_price discriminator
    }
}

#[test]
fn test_read_price() {
    let mut svm = setup();

    // A price exactly at the maximum allowed age is still accepted.
    svm.warp_to_timestamp(MOCK_PUBLISH_TIME + MAXIMUM_PRICE_AGE_SECONDS);

    let price_update = Pubkey::new_unique();
    let price_account =
        price_update_account(price_update, Pubkey::from(PYTH_RECEIVER_PROGRAM_ID));

    let result =
        svm.process_instruction(&read_price_instruction(price_update), &[price_account]);
    result.assert_success();
}

#[test]
fn test_read_price_rejects_stale_price() {
    let mut svm = setup();

    // One second past the maximum age: rejected as stale.
    svm.warp_to_timestamp(MOCK_PUBLISH_TIME + MAXIMUM_PRICE_AGE_SECONDS + 1);

    let price_update = Pubkey::new_unique();
    let price_account =
        price_update_account(price_update, Pubkey::from(PYTH_RECEIVER_PROGRAM_ID));

    let result =
        svm.process_instruction(&read_price_instruction(price_update), &[price_account]);
    result.assert_error(quasar_svm::ProgramError::Custom(
        PythExampleError::PriceTooOld as u32,
    ));
}

#[test]
fn test_read_price_rejects_wrong_owner() {
    let mut svm = setup();

    svm.warp_to_timestamp(MOCK_PUBLISH_TIME);

    // Plausible price bytes, but the account is owned by some random program
    // instead of the Pyth Receiver: the owner constraint must reject it.
    let price_update = Pubkey::new_unique();
    let price_account = price_update_account(price_update, Pubkey::new_unique());

    let result =
        svm.process_instruction(&read_price_instruction(price_update), &[price_account]);
    result.assert_error(quasar_svm::ProgramError::Custom(
        PythExampleError::PriceUpdateNotOwnedByPythReceiver as u32,
    ));
}
