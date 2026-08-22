use {
    anchor_lang::{
        solana_program::instruction::Instruction, Address, InstructionData, ToAccountMetas,
    },
    anchor_v2_testing::{Keypair, LiteSVM, Signer},
    pythexample::MAXIMUM_PRICE_AGE_SECONDS,
    // LiteSVM's get_sysvar wants the host-side Clock, not pinocchio's.
    solana_clock::Clock,
    solana_kite::{create_wallet, send_transaction_from_instructions},
};

/// The `publish_time` baked into the mock price update below.
const MOCK_PUBLISH_TIME: i64 = 1_700_000_000;

/// Pyth Receiver program ID (rec5EKMGg6MxZYaMdyBfgwp4d5rB9T1VQH5pJv5LtFJ)
fn pyth_receiver_program_id() -> anchor_lang::Address {
    pythexample::PYTH_RECEIVER_PROGRAM_ID
}

/// Build mock PriceUpdateV2 account data with Anchor discriminator.
fn build_mock_price_update_account(write_authority: &anchor_lang::Address) -> Vec<u8> {
    // Discriminator: sha256("account:PriceUpdateV2")[..8]
    let discriminator: [u8; 8] = [34, 241, 35, 99, 157, 126, 244, 205];

    let mut data = Vec::with_capacity(133);

    // Discriminator
    data.extend_from_slice(&discriminator);

    // write_authority: Pubkey (32 bytes)
    data.extend_from_slice(write_authority.as_ref());

    // verification_level: Full = borsh enum variant 1
    data.push(1u8);

    // PriceFeedMessage fields:
    // feed_id: [u8; 32]
    let feed_id = [0xEFu8; 32];
    data.extend_from_slice(&feed_id);

    // price: i64 (150.00000000 USD with exponent -8)
    let price: i64 = 15_000_000_000;
    data.extend_from_slice(&price.to_le_bytes());

    // conf: u64
    let conf: u64 = 100_000;
    data.extend_from_slice(&conf.to_le_bytes());

    // exponent: i32
    let exponent: i32 = -8;
    data.extend_from_slice(&exponent.to_le_bytes());

    // publish_time: i64
    data.extend_from_slice(&MOCK_PUBLISH_TIME.to_le_bytes());

    // prev_publish_time: i64
    let prev_publish_time: i64 = 1_699_999_999;
    data.extend_from_slice(&prev_publish_time.to_le_bytes());

    // ema_price: i64
    let ema_price: i64 = 14_900_000_000;
    data.extend_from_slice(&ema_price.to_le_bytes());

    // ema_conf: u64
    let ema_conf: u64 = 120_000;
    data.extend_from_slice(&ema_conf.to_le_bytes());

    // posted_slot: u64
    let posted_slot: u64 = 42;
    data.extend_from_slice(&posted_slot.to_le_bytes());

    data
}

/// Set the test clock so the mock price update is `age_seconds` old.
fn set_clock_to_price_age(svm: &mut LiteSVM, age_seconds: i64) {
    let mut clock: Clock = svm.get_sysvar();
    clock.unix_timestamp = MOCK_PUBLISH_TIME + age_seconds;
    svm.set_sysvar(&clock);
}

fn setup_with_price_account(
    owner: anchor_lang::Address,
) -> (LiteSVM, anchor_v2_testing::Keypair, Keypair) {
    let program_id = pythexample::id();
    let mut svm = anchor_v2_testing::svm();
    let bytes = include_bytes!("../../../target/deploy/pythexample.so");
    svm.add_program(program_id, bytes).unwrap();
    let payer = create_wallet(&mut svm, 10_000_000_000).unwrap();

    // Create a mock PriceUpdateV2 account with the given owner.
    let price_update_key = Keypair::new();
    let account_data = build_mock_price_update_account(&payer.pubkey());
    let rent = svm.minimum_balance_for_rent_exemption(account_data.len());

    svm.set_account(
        price_update_key.pubkey(),
        solana_account::Account {
            lamports: rent,
            data: account_data,
            owner,
            executable: false,
            rent_epoch: 0,
        },
    )
    .unwrap();

    (svm, payer, price_update_key)
}

fn read_price_instruction(price_update: anchor_lang::Address) -> Instruction {
    let ix_data = pythexample::instruction::ReadPrice {}.data();
    let accounts =
        pythexample::accounts::ReadPriceAccountConstraints { price_update }.to_account_metas(None);
    Instruction::new_with_bytes(pythexample::id(), &ix_data, accounts)
}

#[test]
fn test_read_price() {
    let (mut svm, payer, price_update_key) = setup_with_price_account(pyth_receiver_program_id());

    // A price exactly at the maximum allowed age is still accepted.
    set_clock_to_price_age(&mut svm, MAXIMUM_PRICE_AGE_SECONDS);

    let instruction = read_price_instruction(price_update_key.pubkey());
    send_transaction_from_instructions(&mut svm, vec![instruction], &[&payer], &payer.pubkey())
        .unwrap();
}

#[test]
fn test_read_price_rejects_stale_price() {
    let (mut svm, payer, price_update_key) = setup_with_price_account(pyth_receiver_program_id());

    // One second past the maximum age: rejected as stale.
    set_clock_to_price_age(&mut svm, MAXIMUM_PRICE_AGE_SECONDS + 1);

    let instruction = read_price_instruction(price_update_key.pubkey());
    let result =
        send_transaction_from_instructions(&mut svm, vec![instruction], &[&payer], &payer.pubkey());
    assert!(result.is_err(), "a stale price update must be rejected");
}

#[test]
fn test_read_price_rejects_wrong_owner() {
    // Plausible price data, but the account is owned by some random program
    // instead of the Pyth Receiver: Anchor's Account<PriceUpdateV2> owner
    // check must reject it.
    let fake_owner = Keypair::new().pubkey();
    let (mut svm, payer, price_update_key) = setup_with_price_account(fake_owner);

    set_clock_to_price_age(&mut svm, 0);

    let instruction = read_price_instruction(price_update_key.pubkey());
    let result =
        send_transaction_from_instructions(&mut svm, vec![instruction], &[&payer], &payer.pubkey());
    assert!(
        result.is_err(),
        "a price account not owned by the Pyth Receiver must be rejected"
    );
}
