//! QuasarSVM integration tests. They drive the real program instructions
//! end-to-end: initialize the config, open an event, add outcomes, place bets,
//! settle, and claim, asserting on-chain state and token balances at each step.
//!
//! Multi-step flows use `process_instruction_chain`, which runs several
//! instructions atomically over a shared, evolving account set.

extern crate std;

use {
    alloc::vec,
    alloc::vec::Vec,
    quasar_svm::{Account, AccountMeta, Instruction, Pubkey, QuasarSvm},
    solana_program_pack::Pack,
    spl_token_interface::state::{Account as TokenAccount, AccountState, Mint},
    std::println,
};

use crate::state::{BET_SEED, CONFIG_SEED, EVENT_SEED, OUTCOME_SEED, USER_SEED};

const FEE_BPS: u16 = 100; // 1%
const DECIMALS: u8 = 6;
const STARTING_LAMPORTS: u64 = 1_000_000_000;
const STARTING_TOKENS: u64 = 1_000;

fn program_id() -> Pubkey {
    Pubkey::new_from_array(crate::ID.to_bytes())
}

fn setup() -> QuasarSvm {
    let elf = std::fs::read("target/deploy/quasar_betting_market.so").unwrap();
    QuasarSvm::new()
        .with_program(&program_id(), &elf)
        .with_token_program()
}

fn rent_id() -> Pubkey {
    quasar_svm::solana_sdk_ids::sysvar::rent::ID
}
fn token_program_id() -> Pubkey {
    quasar_svm::SPL_TOKEN_PROGRAM_ID
}
fn system_program_id() -> Pubkey {
    quasar_svm::system_program::ID
}

fn signer_account(address: Pubkey) -> Account {
    quasar_svm::token::create_keyed_system_account(&address, STARTING_LAMPORTS)
}

fn empty_account(address: Pubkey) -> Account {
    Account {
        address,
        lamports: 0,
        data: vec![],
        owner: system_program_id(),
        executable: false,
    }
}

fn mint_account(address: Pubkey, authority: Pubkey) -> Account {
    quasar_svm::token::create_keyed_mint_account(
        &address,
        &Mint {
            mint_authority: Some(authority).into(),
            supply: 1_000_000_000,
            decimals: DECIMALS,
            is_initialized: true,
            freeze_authority: None.into(),
        },
    )
}

fn token_account(address: Pubkey, mint: Pubkey, owner: Pubkey, amount: u64) -> Account {
    quasar_svm::token::create_keyed_token_account(
        &address,
        &TokenAccount {
            mint,
            owner,
            amount,
            state: AccountState::Initialized,
            ..TokenAccount::default()
        },
    )
}

fn token_amount(account: &Account) -> u64 {
    TokenAccount::unpack(&account.data).unwrap().amount
}

fn config_pda() -> Pubkey {
    Pubkey::find_program_address(&[CONFIG_SEED], &program_id()).0
}
fn event_pda(event_id: u64) -> Pubkey {
    Pubkey::find_program_address(&[EVENT_SEED, &event_id.to_le_bytes()], &program_id()).0
}
fn vault_pda(event: &Pubkey) -> Pubkey {
    Pubkey::find_program_address(&[b"vault", event.as_ref()], &program_id()).0
}
fn outcome_pda(event: &Pubkey, index: u8) -> Pubkey {
    Pubkey::find_program_address(&[OUTCOME_SEED, event.as_ref(), &[index]], &program_id()).0
}
fn bet_pda(outcome: &Pubkey, bettor: &Pubkey) -> Pubkey {
    Pubkey::find_program_address(&[BET_SEED, outcome.as_ref(), bettor.as_ref()], &program_id()).0
}
fn user_pda(bettor: &Pubkey) -> Pubkey {
    Pubkey::find_program_address(&[USER_SEED, bettor.as_ref()], &program_id()).0
}

// --- Instruction data builders (discriminator byte + args). Strings use
// Quasar's compact wire format: a u8 length prefix then the bytes. ---

fn initialize_config_data(fee_bps: u16, fee_recipient: &Pubkey) -> Vec<u8> {
    let mut data = vec![0u8];
    data.extend_from_slice(&fee_bps.to_le_bytes());
    data.extend_from_slice(fee_recipient.as_ref());
    data
}
fn create_event_data(event_id: u64, description: &str) -> Vec<u8> {
    let mut data = vec![1u8];
    data.extend_from_slice(&event_id.to_le_bytes());
    data.push(description.len() as u8);
    data.extend_from_slice(description.as_bytes());
    data
}
fn add_outcome_data(label: &str) -> Vec<u8> {
    let mut data = vec![2u8];
    data.push(label.len() as u8);
    data.extend_from_slice(label.as_bytes());
    data
}
fn place_bet_data(amount: u64) -> Vec<u8> {
    let mut data = vec![3u8];
    data.extend_from_slice(&amount.to_le_bytes());
    data
}
fn settle_event_data(winning_outcome_index: u8) -> Vec<u8> {
    vec![4u8, winning_outcome_index]
}
fn claim_winnings_data() -> Vec<u8> {
    vec![5u8]
}
fn close_losing_bet_data() -> Vec<u8> {
    vec![6u8]
}
fn cancel_event_data() -> Vec<u8> {
    vec![7u8]
}
fn claim_refund_data() -> Vec<u8> {
    vec![8u8]
}

struct Fixture {
    admin: Pubkey,
    token_mint: Pubkey,
    config: Pubkey,
    fee_recipient: Pubkey,
    fee_recipient_token: Pubkey,
}

fn fixture() -> Fixture {
    let admin = Pubkey::new_unique();
    let fee_recipient = Pubkey::new_unique();
    Fixture {
        admin,
        token_mint: Pubkey::new_unique(),
        config: config_pda(),
        fee_recipient,
        fee_recipient_token: Pubkey::new_unique(),
    }
}

fn initialize_config_ix(fx: &Fixture) -> Instruction {
    Instruction {
        program_id: program_id(),
        accounts: vec![
            AccountMeta::new(fx.admin, true),
            AccountMeta::new_readonly(fx.token_mint, false),
            AccountMeta::new(fx.config, false),
            AccountMeta::new_readonly(rent_id(), false),
            AccountMeta::new_readonly(token_program_id(), false),
            AccountMeta::new_readonly(system_program_id(), false),
        ],
        data: initialize_config_data(FEE_BPS, &fx.fee_recipient),
    }
}

fn create_event_ix(fx: &Fixture, event_id: u64, event: &Pubkey, vault: &Pubkey) -> Instruction {
    Instruction {
        program_id: program_id(),
        accounts: vec![
            AccountMeta::new(fx.admin, true),
            AccountMeta::new(fx.config, false),
            AccountMeta::new_readonly(fx.token_mint, false),
            AccountMeta::new(*event, false),
            AccountMeta::new(*vault, false),
            AccountMeta::new_readonly(rent_id(), false),
            AccountMeta::new_readonly(token_program_id(), false),
            AccountMeta::new_readonly(system_program_id(), false),
        ],
        data: create_event_data(event_id, "Team A vs Team B"),
    }
}

fn add_outcome_ix(fx: &Fixture, event: &Pubkey, outcome: &Pubkey, label: &str) -> Instruction {
    Instruction {
        program_id: program_id(),
        accounts: vec![
            AccountMeta::new(fx.admin, true),
            AccountMeta::new_readonly(fx.config, false),
            AccountMeta::new(*event, false),
            AccountMeta::new(*outcome, false),
            AccountMeta::new_readonly(rent_id(), false),
            AccountMeta::new_readonly(system_program_id(), false),
        ],
        data: add_outcome_data(label),
    }
}

#[allow(clippy::too_many_arguments)]
fn place_bet_ix(
    fx: &Fixture,
    bettor: &Pubkey,
    event: &Pubkey,
    outcome: &Pubkey,
    bettor_token: &Pubkey,
    vault: &Pubkey,
    bet: &Pubkey,
    user: &Pubkey,
    amount: u64,
) -> Instruction {
    Instruction {
        program_id: program_id(),
        accounts: vec![
            AccountMeta::new(*bettor, true),
            AccountMeta::new_readonly(fx.config, false),
            AccountMeta::new_readonly(fx.token_mint, false),
            AccountMeta::new(*event, false),
            AccountMeta::new(*outcome, false),
            AccountMeta::new(*bettor_token, false),
            AccountMeta::new(*vault, false),
            AccountMeta::new(*bet, false),
            AccountMeta::new(*user, false),
            AccountMeta::new_readonly(rent_id(), false),
            AccountMeta::new_readonly(token_program_id(), false),
            AccountMeta::new_readonly(system_program_id(), false),
        ],
        data: place_bet_data(amount),
    }
}

fn settle_event_ix(
    fx: &Fixture,
    event: &Pubkey,
    winning_outcome: &Pubkey,
    vault: &Pubkey,
    winning_outcome_index: u8,
) -> Instruction {
    Instruction {
        program_id: program_id(),
        accounts: vec![
            AccountMeta::new(fx.admin, true),
            AccountMeta::new_readonly(fx.config, false),
            AccountMeta::new_readonly(fx.token_mint, false),
            AccountMeta::new(*event, false),
            AccountMeta::new_readonly(*winning_outcome, false),
            AccountMeta::new(*vault, false),
            AccountMeta::new(fx.fee_recipient_token, false),
            AccountMeta::new_readonly(token_program_id(), false),
        ],
        data: settle_event_data(winning_outcome_index),
    }
}

fn claim_winnings_ix(
    fx: &Fixture,
    bettor: &Pubkey,
    event: &Pubkey,
    bet: &Pubkey,
    user: &Pubkey,
    bettor_token: &Pubkey,
    vault: &Pubkey,
) -> Instruction {
    Instruction {
        program_id: program_id(),
        accounts: vec![
            AccountMeta::new(*bettor, true),
            AccountMeta::new_readonly(fx.token_mint, false),
            AccountMeta::new_readonly(*event, false),
            AccountMeta::new(*bet, false),
            AccountMeta::new(*user, false),
            AccountMeta::new(*bettor_token, false),
            AccountMeta::new(*vault, false),
            AccountMeta::new_readonly(token_program_id(), false),
        ],
        data: claim_winnings_data(),
    }
}

fn close_losing_bet_ix(
    bettor: &Pubkey,
    event: &Pubkey,
    bet: &Pubkey,
    user: &Pubkey,
) -> Instruction {
    Instruction {
        program_id: program_id(),
        accounts: vec![
            AccountMeta::new(*bettor, true),
            AccountMeta::new_readonly(*event, false),
            AccountMeta::new(*bet, false),
            AccountMeta::new(*user, false),
        ],
        data: close_losing_bet_data(),
    }
}

fn claim_refund_ix(
    fx: &Fixture,
    bettor: &Pubkey,
    event: &Pubkey,
    bet: &Pubkey,
    user: &Pubkey,
    bettor_token: &Pubkey,
    vault: &Pubkey,
) -> Instruction {
    Instruction {
        program_id: program_id(),
        accounts: vec![
            AccountMeta::new(*bettor, true),
            AccountMeta::new_readonly(fx.token_mint, false),
            AccountMeta::new_readonly(*event, false),
            AccountMeta::new(*bet, false),
            AccountMeta::new(*user, false),
            AccountMeta::new(*bettor_token, false),
            AccountMeta::new(*vault, false),
            AccountMeta::new_readonly(token_program_id(), false),
        ],
        data: claim_refund_data(),
    }
}

fn cancel_event_ix(fx: &Fixture, event: &Pubkey) -> Instruction {
    Instruction {
        program_id: program_id(),
        accounts: vec![
            AccountMeta::new_readonly(fx.admin, true),
            AccountMeta::new_readonly(fx.config, false),
            AccountMeta::new(*event, false),
        ],
        data: cancel_event_data(),
    }
}

// --- Account field offsets (dense 1-byte discriminator + tight packing). ---
const EVENT_STATUS_OFFSET: usize = 1 + 8 + 1 + 8;
const EVENT_WINNING_INDEX_OFFSET: usize = 1 + 8 + 1 + 8 + 1 + 2;
const EVENT_WINNING_POOL_OFFSET: usize = 1 + 8 + 1 + 8 + 1 + 2 + 1;
const EVENT_DISTRIBUTABLE_OFFSET: usize = 1 + 8 + 1 + 8 + 1 + 2 + 1 + 8;
// User: disc(1) authority(32) bump(1) bet_count(1) ...
const USER_BET_COUNT_OFFSET: usize = 1 + 32 + 1;

fn read_u64(data: &[u8], offset: usize) -> u64 {
    let mut bytes = [0u8; 8];
    bytes.copy_from_slice(&data[offset..offset + 8]);
    u64::from_le_bytes(bytes)
}

const STATUS_SETTLED: u8 = 1;
const STATUS_CANCELLED: u8 = 2;

#[test]
fn test_initialize_config() {
    let mut svm = setup();
    let fx = fixture();

    let accounts = vec![
        signer_account(fx.admin),
        mint_account(fx.token_mint, fx.admin),
        empty_account(fx.config),
    ];

    let result = svm.process_instruction(&initialize_config_ix(&fx), &accounts);
    assert!(result.is_ok(), "initialize_config failed: {:?}", result.raw_result);

    let config = result.account(&fx.config).unwrap();
    assert_eq!(config.data[0], 1, "config discriminator");
    assert_eq!(&config.data[1..33], fx.admin.as_ref(), "admin");
    assert_eq!(&config.data[33..65], fx.token_mint.as_ref(), "token_mint");
    assert_eq!(&config.data[65..97], fx.fee_recipient.as_ref(), "fee_recipient");
    assert_eq!(&config.data[97..99], &FEE_BPS.to_le_bytes(), "fee_bps");

    println!("  INITIALIZE_CONFIG CU: {}", result.compute_units_consumed);
}

/// Full parimutuel flow: two bettors stake on opposing outcomes, the admin
/// settles to the larger pool, the winner claims stake + share of the losing
/// pool (net of the 1% fee), and the loser closes their worthless bet.
///
/// A stakes 100 on outcome 0; B stakes 300 on outcome 1; outcome 1 wins.
/// losing_pool = 100, fee = 1, distributable = 99. B's winnings = 300*99/300 =
/// 99, payout = 399. Fee recipient gets 1. Vault ends empty.
#[test]
fn test_full_lifecycle() {
    let mut svm = setup();
    let fx = fixture();

    let event_id = 1u64;
    let event = event_pda(event_id);
    let vault = vault_pda(&event);
    let outcome0 = outcome_pda(&event, 0);
    let outcome1 = outcome_pda(&event, 1);

    let bettor_a = Pubkey::new_unique();
    let bettor_b = Pubkey::new_unique();
    let token_a = Pubkey::new_unique();
    let token_b = Pubkey::new_unique();
    let user_a = user_pda(&bettor_a);
    let user_b = user_pda(&bettor_b);
    let bet_a = bet_pda(&outcome0, &bettor_a);
    let bet_b = bet_pda(&outcome1, &bettor_b);

    const STAKE_A: u64 = 100;
    const STAKE_B: u64 = 300;
    const FEE: u64 = 1; // ceil? no - floor(100 * 100 / 10000) = 1
    const PAYOUT_B: u64 = STAKE_B + 99; // stake + winnings(99)

    let accounts = vec![
        signer_account(fx.admin),
        mint_account(fx.token_mint, fx.admin),
        empty_account(fx.config),
        empty_account(event),
        empty_account(vault),
        empty_account(outcome0),
        empty_account(outcome1),
        signer_account(bettor_a),
        token_account(token_a, fx.token_mint, bettor_a, STARTING_TOKENS),
        empty_account(user_a),
        empty_account(bet_a),
        signer_account(bettor_b),
        token_account(token_b, fx.token_mint, bettor_b, STARTING_TOKENS),
        empty_account(user_b),
        empty_account(bet_b),
        token_account(fx.fee_recipient_token, fx.token_mint, fx.fee_recipient, 0),
    ];

    let instructions = vec![
        initialize_config_ix(&fx),
        create_event_ix(&fx, event_id, &event, &vault),
        add_outcome_ix(&fx, &event, &outcome0, "Team A"),
        add_outcome_ix(&fx, &event, &outcome1, "Team B"),
        place_bet_ix(&fx, &bettor_a, &event, &outcome0, &token_a, &vault, &bet_a, &user_a, STAKE_A),
        place_bet_ix(&fx, &bettor_b, &event, &outcome1, &token_b, &vault, &bet_b, &user_b, STAKE_B),
        settle_event_ix(&fx, &event, &outcome1, &vault, 1),
        claim_winnings_ix(&fx, &bettor_b, &event, &bet_b, &user_b, &token_b, &vault),
        close_losing_bet_ix(&bettor_a, &event, &bet_a, &user_a),
    ];

    let result = svm.process_instruction_chain(&instructions, &accounts);
    assert!(result.is_ok(), "lifecycle chain failed: {:?}", result.raw_result);

    // Event settled with the recorded figures.
    let event_data = &result.account(&event).unwrap().data;
    assert_eq!(event_data[EVENT_STATUS_OFFSET], STATUS_SETTLED, "status settled");
    assert_eq!(event_data[EVENT_WINNING_INDEX_OFFSET], 1, "winning index");
    assert_eq!(read_u64(event_data, EVENT_WINNING_POOL_OFFSET), STAKE_B, "winning pool");
    assert_eq!(read_u64(event_data, EVENT_DISTRIBUTABLE_OFFSET), 99, "distributable");

    // Token movements.
    assert_eq!(token_amount(result.account(&fx.fee_recipient_token).unwrap()), FEE);
    assert_eq!(
        token_amount(result.account(&token_b).unwrap()),
        STARTING_TOKENS - STAKE_B + PAYOUT_B
    );
    assert_eq!(token_amount(result.account(&token_a).unwrap()), STARTING_TOKENS - STAKE_A);
    assert_eq!(token_amount(result.account(&vault).unwrap()), 0, "vault drained");

    // Both bets closed; user indexes emptied.
    assert_eq!(result.account(&bet_a).map(|a| a.lamports).unwrap_or(0), 0, "bet A closed");
    assert_eq!(result.account(&bet_b).map(|a| a.lamports).unwrap_or(0), 0, "bet B closed");
    assert_eq!(result.account(&user_a).unwrap().data[USER_BET_COUNT_OFFSET], 0);
    assert_eq!(result.account(&user_b).unwrap().data[USER_BET_COUNT_OFFSET], 0);

    println!("  LIFECYCLE CU: {}", result.compute_units_consumed);
}

/// A cancelled event refunds each bettor their exact stake.
#[test]
fn test_cancel_and_refund() {
    let mut svm = setup();
    let fx = fixture();

    let event_id = 1u64;
    let event = event_pda(event_id);
    let vault = vault_pda(&event);
    let outcome0 = outcome_pda(&event, 0);
    let bettor = Pubkey::new_unique();
    let token = Pubkey::new_unique();
    let user = user_pda(&bettor);
    let bet = bet_pda(&outcome0, &bettor);

    const STAKE: u64 = 250;

    let accounts = vec![
        signer_account(fx.admin),
        mint_account(fx.token_mint, fx.admin),
        empty_account(fx.config),
        empty_account(event),
        empty_account(vault),
        empty_account(outcome0),
        signer_account(bettor),
        token_account(token, fx.token_mint, bettor, STARTING_TOKENS),
        empty_account(user),
        empty_account(bet),
    ];

    let instructions = vec![
        initialize_config_ix(&fx),
        create_event_ix(&fx, event_id, &event, &vault),
        add_outcome_ix(&fx, &event, &outcome0, "Only"),
        place_bet_ix(&fx, &bettor, &event, &outcome0, &token, &vault, &bet, &user, STAKE),
        cancel_event_ix(&fx, &event),
        claim_refund_ix(&fx, &bettor, &event, &bet, &user, &token, &vault),
    ];

    let result = svm.process_instruction_chain(&instructions, &accounts);
    assert!(result.is_ok(), "cancel/refund chain failed: {:?}", result.raw_result);

    assert_eq!(result.account(&event).unwrap().data[EVENT_STATUS_OFFSET], STATUS_CANCELLED);
    // The bettor got their exact stake back and the bet closed.
    assert_eq!(token_amount(result.account(&token).unwrap()), STARTING_TOKENS);
    assert_eq!(token_amount(result.account(&vault).unwrap()), 0);
    assert_eq!(result.account(&bet).map(|a| a.lamports).unwrap_or(0), 0, "bet closed");

    println!("  CANCEL/REFUND CU: {}", result.compute_units_consumed);
}

/// Only the config admin may open an event.
#[test]
fn test_create_event_rejects_non_admin() {
    let mut svm = setup();
    let fx = fixture();
    let attacker = Pubkey::new_unique();

    let event_id = 1u64;
    let event = event_pda(event_id);
    let vault = vault_pda(&event);

    // Init config with the real admin, then have an attacker try create_event.
    let mut attacker_fx = fixture();
    attacker_fx.admin = attacker;
    attacker_fx.token_mint = fx.token_mint;
    attacker_fx.config = fx.config;

    let accounts = vec![
        signer_account(fx.admin),
        signer_account(attacker),
        mint_account(fx.token_mint, fx.admin),
        empty_account(fx.config),
        empty_account(event),
        empty_account(vault),
    ];

    let instructions = vec![
        initialize_config_ix(&fx),
        create_event_ix(&attacker_fx, event_id, &event, &vault),
    ];
    let result = svm.process_instruction_chain(&instructions, &accounts);
    assert!(
        !result.is_ok(),
        "create_event must reject a signer who is not the config admin"
    );
}
