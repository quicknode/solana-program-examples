//! Quasar-SVM tests for the asset-leasing program.
//!
//! Covers the full lifecycle: listing, taking, lease fee streaming, top-ups,
//! early return, keeper liquidation via a mocked Pyth `PriceUpdateV2`
//! account, and lessor-initiated default recovery after expiry.
//!
//! Each test constructs a fresh `QuasarSvm`, synthesises the minimal set
//! of accounts that handler needs (mints, token accounts, the existing
//! lease state where relevant), and submits a manually-assembled
//! instruction. State updates are read straight back out of the SVM.

extern crate std;

use {
    alloc::{vec, vec::Vec},
    quasar_svm::{Account, Instruction, Pubkey, QuasarSvm},
    solana_instruction::AccountMeta,
    spl_token_interface::state::{Account as TokenAccount, AccountState, Mint},
    std::println,
};

use crate::{
    constants::{COLLATERAL_VAULT_SEED, LEASED_VAULT_SEED, LEASE_SEED},
    state::LeaseStatus,
};

// ---------------------------------------------------------------------------
// Shared test constants
// ---------------------------------------------------------------------------

/// USDC-style decimals keep the arithmetic readable in asserts.
const DECIMALS: u8 = 6;

/// 100 leased tokens at 6 decimals.
const LEASED_AMOUNT: u64 = 100_000_000;
/// 200 collateral tokens at 6 decimals.
const REQUIRED_COLLATERAL: u64 = 200_000_000;
const LEASE_FEE_PER_SECOND: u64 = 10;
/// 24 hours.
const DURATION_SECONDS: i64 = 60 * 60 * 24;
/// 120% maintenance margin, in basis points.
const MAINTENANCE_MARGIN_BASIS_POINTS: u16 = 12_000;
/// 5% keeper bounty, in basis points.
const LIQUIDATION_BOUNTY_BASIS_POINTS: u16 = 500;
/// Arbitrary 32-byte Pyth feed id the tests pin their leases to.
const FEED_ID: [u8; 32] = [0xAB; 32];

/// LiteSVM's default clock starts at epoch 0; anchoring at a recent-ish
/// real timestamp keeps lease fee math free of sign-weirdness without any
/// tests having to special-case `start_timestamp = 0`.
const DEFAULT_TIMESTAMP: i64 = 1_700_000_000;

/// Starting wallet balance for lessor and lessee token accounts.
const STARTING_BALANCE: u64 = 1_000_000_000;

/// Pyth receiver program id on mainnet/devnet. Matches
/// [`crate::instructions::liquidate::PYTH_RECEIVER_PROGRAM_ID`].
fn pyth_receiver_pubkey() -> Pubkey {
    Pubkey::from([
        12, 183, 250, 187, 82, 247, 166, 72, 187, 91, 49, 125, 154, 1, 139, 144, 87, 203, 2, 71,
        116, 250, 254, 1, 230, 196, 223, 152, 204, 56, 88, 129,
    ])
}

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

fn setup() -> QuasarSvm {
    let elf = std::fs::read("target/deploy/quasar_asset_leasing.so")
        .expect("build the program with `quasar build` before running tests");
    let mut svm = QuasarSvm::new()
        .with_program(&crate::ID, &elf)
        .with_token_program();
    svm.warp_to_timestamp(DEFAULT_TIMESTAMP);
    svm
}

fn signer(address: Pubkey) -> Account {
    quasar_svm::token::create_keyed_system_account(&address, 1_000_000_000)
}

fn empty(address: Pubkey) -> Account {
    Account {
        address,
        lamports: 0,
        data: vec![],
        owner: quasar_svm::system_program::ID,
        executable: false,
    }
}

fn mint(address: Pubkey, authority: Pubkey) -> Account {
    quasar_svm::token::create_keyed_mint_account(
        &address,
        &Mint {
            mint_authority: Some(authority).into(),
            supply: STARTING_BALANCE * 4,
            decimals: DECIMALS,
            is_initialized: true,
            freeze_authority: None.into(),
        },
    )
}

fn token(address: Pubkey, mint: Pubkey, owner: Pubkey, amount: u64) -> Account {
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

/// Byte offsets for reading fields out of a serialised `Lease` account.
/// Layout (after the `#[account(discriminator = 1)]` macro lowers fields
/// to pod types): 1 disc + 8 lease_id + 32 lessor + 32 lessee + 32
/// leased_mint + 8 leased_amount + 32 collateral_mint + 8 collateral_amount
/// + 8 required_collateral + 8 lease_fee_per_second + 8 duration + 8 start_timestamp +
/// 8 end_timestamp + 8 last_paid_timestamp + 2 margin_basis_points + 2 bounty_basis_points + 32
/// feed_id + 4 status/bumps = 249 bytes.
mod lease_offsets {
    pub const COLLATERAL_AMOUNT: usize = 1 + 8 + 32 + 32 + 32 + 8 + 32;
    pub const LAST_PAID_TIMESTAMP: usize = COLLATERAL_AMOUNT + 8 + 8 + 8 + 8 + 8 + 8;
    pub const STATUS: usize = LAST_PAID_TIMESTAMP + 8 + 2 + 2 + 32;
}

fn read_collateral_amount(data: &[u8]) -> u64 {
    u64::from_le_bytes(
        data[lease_offsets::COLLATERAL_AMOUNT..lease_offsets::COLLATERAL_AMOUNT + 8]
            .try_into()
            .unwrap(),
    )
}

fn read_status(data: &[u8]) -> u8 {
    data[lease_offsets::STATUS]
}

fn read_token_amount(account: &Account) -> u64 {
    u64::from_le_bytes(account.data[64..72].try_into().unwrap())
}

// ---------------------------------------------------------------------------
// program-derived address derivations (mirror the program's `#[account(seeds = ...)]`)
// ---------------------------------------------------------------------------

fn lease_program_derived_address(lessor: &Pubkey) -> (Pubkey, u8) {
    Pubkey::find_program_address(&[LEASE_SEED, lessor.as_ref()], &crate::ID)
}

fn leased_vault_program_derived_address(lease: &Pubkey) -> (Pubkey, u8) {
    Pubkey::find_program_address(&[LEASED_VAULT_SEED, lease.as_ref()], &crate::ID)
}

fn collateral_vault_program_derived_address(lease: &Pubkey) -> (Pubkey, u8) {
    Pubkey::find_program_address(&[COLLATERAL_VAULT_SEED, lease.as_ref()], &crate::ID)
}

// ---------------------------------------------------------------------------
// Instruction builders
// ---------------------------------------------------------------------------

#[allow(clippy::too_many_arguments)]
fn build_create_lease_data(
    lease_id: u64,
    leased_amount: u64,
    required_collateral_amount: u64,
    lease_fee_per_second: u64,
    duration_seconds: i64,
    maintenance_margin_basis_points: u16,
    liquidation_bounty_basis_points: u16,
    feed_id: [u8; 32],
) -> Vec<u8> {
    let mut data = vec![0u8]; // discriminator for create_lease
    data.extend_from_slice(&lease_id.to_le_bytes());
    data.extend_from_slice(&leased_amount.to_le_bytes());
    data.extend_from_slice(&required_collateral_amount.to_le_bytes());
    data.extend_from_slice(&lease_fee_per_second.to_le_bytes());
    data.extend_from_slice(&duration_seconds.to_le_bytes());
    data.extend_from_slice(&maintenance_margin_basis_points.to_le_bytes());
    data.extend_from_slice(&liquidation_bounty_basis_points.to_le_bytes());
    data.extend_from_slice(&feed_id);
    data
}

// ---------------------------------------------------------------------------
// Scenario — a fresh SVM + the set of pubkeys every test needs
// ---------------------------------------------------------------------------

struct Scenario {
    lessor: Pubkey,
    lessee: Pubkey,
    keeper: Pubkey,
    leased_mint: Pubkey,
    collateral_mint: Pubkey,
    /// Pre-created lessor token account for the leased mint, starts at
    /// `STARTING_BALANCE`.
    lessor_leased_token_account: Pubkey,
    /// Lessor's collateral associated token account, starts empty.
    lessor_collateral_token_account: Pubkey,
    /// Lessee's collateral associated token account, starts at `STARTING_BALANCE`.
    lessee_collateral_token_account: Pubkey,
    /// Lessee's leased associated token account, starts empty.
    lessee_leased_token_account: Pubkey,
    /// Keeper's collateral associated token account, starts empty — bounty destination.
    keeper_collateral_token_account: Pubkey,
    lease: Pubkey,
    leased_vault: Pubkey,
    collateral_vault: Pubkey,
}

fn make_scenario() -> (QuasarSvm, Scenario) {
    let svm = setup();
    let lessor = Pubkey::new_unique();
    let lessee = Pubkey::new_unique();
    let keeper = Pubkey::new_unique();
    let leased_mint = Pubkey::new_unique();
    let collateral_mint = Pubkey::new_unique();
    let lessor_leased_token_account = Pubkey::new_unique();
    let lessor_collateral_token_account = Pubkey::new_unique();
    let lessee_collateral_token_account = Pubkey::new_unique();
    let lessee_leased_token_account = Pubkey::new_unique();
    let keeper_collateral_token_account = Pubkey::new_unique();
    let (lease, _lease_bump) = lease_program_derived_address(&lessor);
    let (leased_vault, _leased_vault_bump) = leased_vault_program_derived_address(&lease);
    let (collateral_vault, _collateral_vault_bump) = collateral_vault_program_derived_address(&lease);
    let scenario = Scenario {
        lessor,
        lessee,
        keeper,
        leased_mint,
        collateral_mint,
        lessor_leased_token_account,
        lessor_collateral_token_account,
        lessee_collateral_token_account,
        lessee_leased_token_account,
        keeper_collateral_token_account,
        lease,
        leased_vault,
        collateral_vault,
    };
    (svm, scenario)
}

// ---------------------------------------------------------------------------
// Instruction assemblers — one per handler, returning `(Instruction,
// Vec<Account>)` pairs ready to hand to `process_instruction`.
//
// The `accounts` vector order matches the order of fields in the matching
// `#[derive(Accounts)]` struct, which is also the order the handler reads
// them from. Off-by-one errors show up as ownership / signer failures,
// never as silent misbehaviour.
// ---------------------------------------------------------------------------

#[allow(clippy::too_many_arguments)]
fn create_lease_call(scenario: &Scenario, lease_id: u64) -> (Instruction, Vec<Account>) {
    // `init + seeds` fields self-sign via `invoke_signed` inside the
    // program, so only the lessor (index 0) is a true signer here.
    let instruction = Instruction {
        program_id: crate::ID,
        accounts: vec![
            AccountMeta::new(scenario.lessor, true),
            AccountMeta::new_readonly(scenario.leased_mint, false),
            AccountMeta::new_readonly(scenario.collateral_mint, false),
            AccountMeta::new(scenario.lessor_leased_token_account, false),
            AccountMeta::new(scenario.lease, false),
            AccountMeta::new(scenario.leased_vault, false),
            AccountMeta::new(scenario.collateral_vault, false),
            AccountMeta::new_readonly(quasar_svm::solana_sdk_ids::sysvar::rent::ID, false),
            AccountMeta::new_readonly(quasar_svm::SPL_TOKEN_PROGRAM_ID, false),
            AccountMeta::new_readonly(quasar_svm::system_program::ID, false),
        ],
        data: build_create_lease_data(
            lease_id,
            LEASED_AMOUNT,
            REQUIRED_COLLATERAL,
            LEASE_FEE_PER_SECOND,
            DURATION_SECONDS,
            MAINTENANCE_MARGIN_BASIS_POINTS,
            LIQUIDATION_BOUNTY_BASIS_POINTS,
            FEED_ID,
        ),
    };

    let accounts = vec![
        signer(scenario.lessor),
        mint(scenario.leased_mint, scenario.lessor),
        mint(scenario.collateral_mint, scenario.lessor),
        token(scenario.lessor_leased_token_account, scenario.leased_mint, scenario.lessor, STARTING_BALANCE),
        empty(scenario.lease),
        empty(scenario.leased_vault),
        empty(scenario.collateral_vault),
    ];

    (instruction, accounts)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[test]
fn create_lease_locks_tokens_and_lists() {
    let (mut svm, scenario) = make_scenario();
    let (instruction, accounts) = create_lease_call(&scenario, 1);
    let result = svm.process_instruction(&instruction, &accounts);
    assert!(result.is_ok(), "create_lease failed: {:?}", result.raw_result);

    // Lease created, vaults initialised.
    let lease_account = result.account(&scenario.lease).expect("lease program-derived address missing");
    assert_eq!(lease_account.owner, crate::ID);
    assert_eq!(read_status(&lease_account.data), LeaseStatus::Listed as u8);

    // Leased tokens escrowed; lessor balance dropped.
    let leased_vault = result.account(&scenario.leased_vault).unwrap();
    assert_eq!(read_token_amount(leased_vault), LEASED_AMOUNT);
    let lessor_token_account = result.account(&scenario.lessor_leased_token_account).unwrap();
    assert_eq!(read_token_amount(lessor_token_account), STARTING_BALANCE - LEASED_AMOUNT);

    // Collateral vault exists, empty.
    let collateral_vault = result.account(&scenario.collateral_vault).unwrap();
    assert_eq!(read_token_amount(collateral_vault), 0);

    println!("  CREATE CU: {}", result.compute_units_consumed);
}

/// Second form of `create_lease` that lets a test swap the mint addresses
/// — used to exercise the same-mint rejection path.
#[allow(clippy::too_many_arguments)]
fn create_lease_call_with_mints(
    scenario: &Scenario,
    lease_id: u64,
    leased_mint: Pubkey,
    collateral_mint: Pubkey,
) -> (Instruction, Vec<Account>) {
    let instruction = Instruction {
        program_id: crate::ID,
        accounts: vec![
            AccountMeta::new(scenario.lessor, true),
            AccountMeta::new_readonly(leased_mint, false),
            AccountMeta::new_readonly(collateral_mint, false),
            AccountMeta::new(scenario.lessor_leased_token_account, false),
            AccountMeta::new(scenario.lease, false),
            AccountMeta::new(scenario.leased_vault, false),
            AccountMeta::new(scenario.collateral_vault, false),
            AccountMeta::new_readonly(quasar_svm::solana_sdk_ids::sysvar::rent::ID, false),
            AccountMeta::new_readonly(quasar_svm::SPL_TOKEN_PROGRAM_ID, false),
            AccountMeta::new_readonly(quasar_svm::system_program::ID, false),
        ],
        data: build_create_lease_data(
            lease_id,
            LEASED_AMOUNT,
            REQUIRED_COLLATERAL,
            LEASE_FEE_PER_SECOND,
            DURATION_SECONDS,
            MAINTENANCE_MARGIN_BASIS_POINTS,
            LIQUIDATION_BOUNTY_BASIS_POINTS,
            FEED_ID,
        ),
    };
    let accounts = vec![
        signer(scenario.lessor),
        mint(leased_mint, scenario.lessor),
        mint(collateral_mint, scenario.lessor),
        token(scenario.lessor_leased_token_account, leased_mint, scenario.lessor, STARTING_BALANCE),
        empty(scenario.lease),
        empty(scenario.leased_vault),
        empty(scenario.collateral_vault),
    ];
    (instruction, accounts)
}

/// Pyth `PriceUpdateV2` body with only the fields liquidate actually reads
/// populated; everything else is zeroed.
fn build_price_update_data(
    feed_id: [u8; 32],
    price: i64,
    exponent: i32,
    publish_time: i64,
) -> Vec<u8> {
    // 8 disc + 32 write_authority + 1 verification_level + 32 feed_id +
    // 8 price + 8 conf + 4 exponent + 8 publish_time + 8 prev_publish_time +
    // 8 ema_price + 8 ema_conf + 8 posted_slot = 141 bytes.
    const TOTAL_LEN: usize = 141;
    const PRICE_UPDATE_V2_DISCRIMINATOR: [u8; 8] = [34, 241, 35, 99, 157, 126, 244, 205];
    let mut data = Vec::with_capacity(TOTAL_LEN);
    data.extend_from_slice(&PRICE_UPDATE_V2_DISCRIMINATOR);
    data.extend_from_slice(&[0u8; 32]); // write_authority
    data.push(1); // verification_level = Full
    data.extend_from_slice(&feed_id);
    data.extend_from_slice(&price.to_le_bytes());
    data.extend_from_slice(&0u64.to_le_bytes()); // conf
    data.extend_from_slice(&exponent.to_le_bytes());
    data.extend_from_slice(&publish_time.to_le_bytes());
    data.extend_from_slice(&publish_time.to_le_bytes()); // prev_publish_time
    data.extend_from_slice(&0i64.to_le_bytes()); // ema_price
    data.extend_from_slice(&0u64.to_le_bytes()); // ema_conf
    data.extend_from_slice(&0u64.to_le_bytes()); // posted_slot
    data
}

fn install_price_update(
    svm: &mut QuasarSvm,
    address: Pubkey,
    feed_id: [u8; 32],
    price: i64,
    exponent: i32,
    publish_time: i64,
) {
    let data = build_price_update_data(feed_id, price, exponent, publish_time);
    svm.set_account(Account {
        address,
        lamports: 10_000_000,
        data,
        owner: pyth_receiver_pubkey(),
        executable: false,
    });
}

fn take_lease_call(scenario: &Scenario) -> (Instruction, Vec<Account>) {
    let instruction = Instruction {
        program_id: crate::ID,
        accounts: vec![
            AccountMeta::new(scenario.lessee, true),
            AccountMeta::new_readonly(scenario.lessor, false),
            AccountMeta::new(scenario.lease, false),
            AccountMeta::new_readonly(scenario.leased_mint, false),
            AccountMeta::new_readonly(scenario.collateral_mint, false),
            AccountMeta::new(scenario.leased_vault, false),
            AccountMeta::new(scenario.collateral_vault, false),
            AccountMeta::new(scenario.lessee_collateral_token_account, false),
            AccountMeta::new(scenario.lessee_leased_token_account, false),
            AccountMeta::new_readonly(quasar_svm::SPL_TOKEN_PROGRAM_ID, false),
        ],
        data: vec![1u8], // discriminator = take_lease
    };
    let accounts = vec![
        signer(scenario.lessee),
        empty(scenario.lessor),
        // `lease` is sourced from the SVM database, already pre-installed.
        mint(scenario.leased_mint, scenario.lessor),
        mint(scenario.collateral_mint, scenario.lessor),
        // `leased_vault` and `collateral_vault` similarly pre-installed.
        token(
            scenario.lessee_collateral_token_account,
            scenario.collateral_mint,
            scenario.lessee,
            STARTING_BALANCE,
        ),
        token(scenario.lessee_leased_token_account, scenario.leased_mint, scenario.lessee, 0),
    ];
    (instruction, accounts)
}

fn pay_lease_fee_call(scenario: &Scenario) -> (Instruction, Vec<Account>) {
    let instruction = Instruction {
        program_id: crate::ID,
        accounts: vec![
            AccountMeta::new(scenario.lessee, true),
            AccountMeta::new_readonly(scenario.lessor, false),
            AccountMeta::new(scenario.lease, false),
            AccountMeta::new_readonly(scenario.collateral_mint, false),
            AccountMeta::new(scenario.collateral_vault, false),
            AccountMeta::new(scenario.lessor_collateral_token_account, false),
            AccountMeta::new_readonly(quasar_svm::SPL_TOKEN_PROGRAM_ID, false),
        ],
        data: vec![2u8],
    };
    let accounts = vec![
        signer(scenario.lessee),
        empty(scenario.lessor),
        mint(scenario.collateral_mint, scenario.lessor),
        token(scenario.lessor_collateral_token_account, scenario.collateral_mint, scenario.lessor, 0),
    ];
    (instruction, accounts)
}

fn top_up_call(scenario: &Scenario, amount: u64) -> (Instruction, Vec<Account>) {
    let mut data = vec![3u8];
    data.extend_from_slice(&amount.to_le_bytes());
    let instruction = Instruction {
        program_id: crate::ID,
        accounts: vec![
            AccountMeta::new(scenario.lessee, true),
            AccountMeta::new_readonly(scenario.lessor, false),
            AccountMeta::new(scenario.lease, false),
            AccountMeta::new_readonly(scenario.collateral_mint, false),
            AccountMeta::new(scenario.collateral_vault, false),
            AccountMeta::new(scenario.lessee_collateral_token_account, false),
            AccountMeta::new_readonly(quasar_svm::SPL_TOKEN_PROGRAM_ID, false),
        ],
        data,
    };
    let accounts = vec![
        signer(scenario.lessee),
        empty(scenario.lessor),
        mint(scenario.collateral_mint, scenario.lessor),
    ];
    (instruction, accounts)
}

fn return_lease_call(scenario: &Scenario) -> (Instruction, Vec<Account>) {
    let instruction = Instruction {
        program_id: crate::ID,
        accounts: vec![
            AccountMeta::new(scenario.lessee, true),
            AccountMeta::new(scenario.lessor, false),
            AccountMeta::new(scenario.lease, false),
            AccountMeta::new_readonly(scenario.leased_mint, false),
            AccountMeta::new_readonly(scenario.collateral_mint, false),
            AccountMeta::new(scenario.leased_vault, false),
            AccountMeta::new(scenario.collateral_vault, false),
            AccountMeta::new(scenario.lessee_leased_token_account, false),
            AccountMeta::new(scenario.lessee_collateral_token_account, false),
            AccountMeta::new(scenario.lessor_leased_token_account, false),
            AccountMeta::new(scenario.lessor_collateral_token_account, false),
            AccountMeta::new_readonly(quasar_svm::SPL_TOKEN_PROGRAM_ID, false),
        ],
        data: vec![4u8],
    };
    let accounts = vec![
        signer(scenario.lessee),
        empty(scenario.lessor),
        mint(scenario.leased_mint, scenario.lessor),
        mint(scenario.collateral_mint, scenario.lessor),
        token(scenario.lessor_collateral_token_account, scenario.collateral_mint, scenario.lessor, 0),
    ];
    (instruction, accounts)
}

fn liquidate_call(scenario: &Scenario, price_update: Pubkey) -> (Instruction, Vec<Account>) {
    let instruction = Instruction {
        program_id: crate::ID,
        accounts: vec![
            AccountMeta::new(scenario.keeper, true),
            AccountMeta::new(scenario.lessor, false),
            AccountMeta::new(scenario.lease, false),
            AccountMeta::new_readonly(scenario.leased_mint, false),
            AccountMeta::new_readonly(scenario.collateral_mint, false),
            AccountMeta::new(scenario.leased_vault, false),
            AccountMeta::new(scenario.collateral_vault, false),
            AccountMeta::new(scenario.lessor_collateral_token_account, false),
            AccountMeta::new(scenario.keeper_collateral_token_account, false),
            AccountMeta::new_readonly(price_update, false),
            AccountMeta::new_readonly(quasar_svm::SPL_TOKEN_PROGRAM_ID, false),
        ],
        data: vec![5u8],
    };
    let accounts = vec![
        signer(scenario.keeper),
        empty(scenario.lessor),
        mint(scenario.leased_mint, scenario.lessor),
        mint(scenario.collateral_mint, scenario.lessor),
        token(scenario.lessor_collateral_token_account, scenario.collateral_mint, scenario.lessor, 0),
        token(scenario.keeper_collateral_token_account, scenario.collateral_mint, scenario.keeper, 0),
    ];
    (instruction, accounts)
}

fn close_expired_call(scenario: &Scenario) -> (Instruction, Vec<Account>) {
    let instruction = Instruction {
        program_id: crate::ID,
        accounts: vec![
            AccountMeta::new(scenario.lessor, true),
            AccountMeta::new(scenario.lease, false),
            AccountMeta::new_readonly(scenario.leased_mint, false),
            AccountMeta::new_readonly(scenario.collateral_mint, false),
            AccountMeta::new(scenario.leased_vault, false),
            AccountMeta::new(scenario.collateral_vault, false),
            AccountMeta::new(scenario.lessor_leased_token_account, false),
            AccountMeta::new(scenario.lessor_collateral_token_account, false),
            AccountMeta::new_readonly(quasar_svm::SPL_TOKEN_PROGRAM_ID, false),
        ],
        data: vec![6u8],
    };
    let accounts = vec![
        signer(scenario.lessor),
        mint(scenario.leased_mint, scenario.lessor),
        mint(scenario.collateral_mint, scenario.lessor),
        token(
            scenario.lessor_leased_token_account,
            scenario.leased_mint,
            scenario.lessor,
            STARTING_BALANCE - LEASED_AMOUNT,
        ),
        token(scenario.lessor_collateral_token_account, scenario.collateral_mint, scenario.lessor, 0),
    ];
    (instruction, accounts)
}

/// After a successful `create_lease`, install the resulting vault + lease
/// state in the SVM database so the next handler call has something to
/// read from. Copies the authentic onchain bytes (discriminator, token
/// amounts, lease fields) straight out of the previous execution result.
fn commit_state<'a>(
    svm: &mut QuasarSvm,
    result: &'a quasar_svm::ExecutionResult,
    addresses: &[Pubkey],
) {
    for address in addresses {
        if let Some(account) = result.account(address) {
            svm.set_account(Account {
                address: *address,
                lamports: account.lamports,
                data: account.data.clone(),
                owner: account.owner,
                executable: account.executable,
            });
        }
    }
}

#[test]
fn take_lease_posts_collateral_and_delivers_tokens() {
    let (mut svm, scenario) = make_scenario();

    // Run create_lease and commit its output (lease + both vaults).
    let (create_instruction, create_accounts) = create_lease_call(&scenario, 2);
    let create_result = svm.process_instruction(&create_instruction, &create_accounts);
    assert!(create_result.is_ok(), "create_lease failed: {:?}", create_result.raw_result);
    commit_state(
        &mut svm,
        &create_result,
        &[scenario.lease, scenario.leased_vault, scenario.collateral_vault, scenario.lessor_leased_token_account],
    );

    // Now the lessee takes it.
    let (take_instruction, take_accounts) = take_lease_call(&scenario);
    let take_result = svm.process_instruction(&take_instruction, &take_accounts);
    assert!(take_result.is_ok(), "take_lease failed: {:?}", take_result.raw_result);

    // Leased vault drained into the lessee.
    assert_eq!(
        read_token_amount(take_result.account(&scenario.leased_vault).unwrap()),
        0
    );
    assert_eq!(
        read_token_amount(take_result.account(&scenario.lessee_leased_token_account).unwrap()),
        LEASED_AMOUNT
    );
    // Collateral moved from the lessee into the collateral vault.
    assert_eq!(
        read_token_amount(take_result.account(&scenario.collateral_vault).unwrap()),
        REQUIRED_COLLATERAL
    );
    assert_eq!(
        read_token_amount(take_result.account(&scenario.lessee_collateral_token_account).unwrap()),
        STARTING_BALANCE - REQUIRED_COLLATERAL
    );
    // Lease transitioned Listed -> Active.
    assert_eq!(
        read_status(&take_result.account(&scenario.lease).unwrap().data),
        LeaseStatus::Active as u8
    );
}

/// Helper: run create + take atomically and commit all resulting state so
/// the next call starts from an `Active` lease.
fn make_and_take(svm: &mut QuasarSvm, scenario: &Scenario) {
    let (create_instruction, create_accounts) = create_lease_call(scenario, 1);
    let create_result = svm.process_instruction(&create_instruction, &create_accounts);
    assert!(create_result.is_ok(), "create_lease failed: {:?}", create_result.raw_result);
    commit_state(
        svm,
        &create_result,
        &[scenario.lease, scenario.leased_vault, scenario.collateral_vault, scenario.lessor_leased_token_account],
    );

    let (take_instruction, take_accounts) = take_lease_call(scenario);
    let take_result = svm.process_instruction(&take_instruction, &take_accounts);
    assert!(take_result.is_ok(), "take_lease failed: {:?}", take_result.raw_result);
    commit_state(
        svm,
        &take_result,
        &[
            scenario.lease,
            scenario.leased_vault,
            scenario.collateral_vault,
            scenario.lessee_collateral_token_account,
            scenario.lessee_leased_token_account,
        ],
    );
}

#[test]
fn pay_lease_fee_streams_collateral_by_elapsed_time() {
    let (mut svm, scenario) = make_scenario();
    make_and_take(&mut svm, &scenario);

    // Advance clock by 2 minutes and pay the lease fee.
    let elapsed: i64 = 120;
    svm.warp_to_timestamp(DEFAULT_TIMESTAMP + elapsed);
    let (pay_instruction, pay_accounts) = pay_lease_fee_call(&scenario);
    let result = svm.process_instruction(&pay_instruction, &pay_accounts);
    assert!(result.is_ok(), "pay_lease_fee failed: {:?}", result.raw_result);

    let expected_lease_fees = (elapsed as u64) * LEASE_FEE_PER_SECOND;
    assert_eq!(
        read_token_amount(result.account(&scenario.lessor_collateral_token_account).unwrap()),
        expected_lease_fees
    );
    assert_eq!(
        read_token_amount(result.account(&scenario.collateral_vault).unwrap()),
        REQUIRED_COLLATERAL - expected_lease_fees
    );
}

#[test]
fn top_up_collateral_increases_vault_balance() {
    let (mut svm, scenario) = make_scenario();
    make_and_take(&mut svm, &scenario);

    let top_up_amount: u64 = 50_000_000;
    let (instruction, accounts) = top_up_call(&scenario, top_up_amount);
    let result = svm.process_instruction(&instruction, &accounts);
    assert!(result.is_ok(), "top_up failed: {:?}", result.raw_result);

    assert_eq!(
        read_token_amount(result.account(&scenario.collateral_vault).unwrap()),
        REQUIRED_COLLATERAL + top_up_amount
    );
    // Collateral amount on the lease bumps too.
    assert_eq!(
        read_collateral_amount(&result.account(&scenario.lease).unwrap().data),
        REQUIRED_COLLATERAL + top_up_amount
    );
}

#[test]
fn return_lease_refunds_unused_collateral() {
    let (mut svm, scenario) = make_scenario();
    make_and_take(&mut svm, &scenario);

    // Lessee returns 10 minutes in, for a 24h lease.
    let elapsed: i64 = 600;
    svm.warp_to_timestamp(DEFAULT_TIMESTAMP + elapsed);

    let (instruction, accounts) = return_lease_call(&scenario);
    let result = svm.process_instruction(&instruction, &accounts);
    assert!(result.is_ok(), "return_lease failed: {:?}", result.raw_result);

    let lease_fee_paid = (elapsed as u64) * LEASE_FEE_PER_SECOND;
    let refund_expected = REQUIRED_COLLATERAL - lease_fee_paid;

    // Lessor got the full leased amount back.
    assert_eq!(
        read_token_amount(result.account(&scenario.lessor_leased_token_account).unwrap()),
        STARTING_BALANCE
    );
    // Lessor received the accrued lease fees.
    assert_eq!(
        read_token_amount(result.account(&scenario.lessor_collateral_token_account).unwrap()),
        lease_fee_paid
    );
    // Lessee got unused-time collateral back.
    assert_eq!(
        read_token_amount(result.account(&scenario.lessee_collateral_token_account).unwrap()),
        STARTING_BALANCE - REQUIRED_COLLATERAL + refund_expected
    );

    // Both vaults closed — the SVM keeps the account record but with
    // lamports=0 / data empty. We check lamports drained rather than
    // .is_none(), which is stricter than needed.
    assert_eq!(
        result.account(&scenario.leased_vault).map(|a| a.lamports).unwrap_or(0),
        0
    );
    assert_eq!(
        result.account(&scenario.collateral_vault).map(|a| a.lamports).unwrap_or(0),
        0
    );
}

#[test]
fn liquidate_seizes_collateral_on_price_drop() {
    let (mut svm, scenario) = make_scenario();
    make_and_take(&mut svm, &scenario);

    // Let 300 s of lease fees accrue so the handler settles lease fees *and* bounty
    // on the same vault balance.
    let elapsed: i64 = 300;
    let now_timestamp = DEFAULT_TIMESTAMP + elapsed;
    svm.warp_to_timestamp(now_timestamp);

    // Price 4.0 with exponent 0 — debt = 400 collateral vs. 200 held,
    // ratio 50% ≪ 120% margin.
    let price_update = Pubkey::new_unique();
    install_price_update(&mut svm, price_update, FEED_ID, 4, 0, now_timestamp);

    let (instruction, accounts) = liquidate_call(&scenario, price_update);
    let result = svm.process_instruction(&instruction, &accounts);
    assert!(result.is_ok(), "liquidate failed: {:?}", result.raw_result);

    let lease_fee_paid = (elapsed as u64) * LEASE_FEE_PER_SECOND;
    let remaining_after_lease_fees = REQUIRED_COLLATERAL - lease_fee_paid;
    let bounty = remaining_after_lease_fees * (LIQUIDATION_BOUNTY_BASIS_POINTS as u64) / 10_000;
    let lessor_share = remaining_after_lease_fees - bounty;

    assert_eq!(
        read_token_amount(result.account(&scenario.lessor_collateral_token_account).unwrap()),
        lease_fee_paid + lessor_share
    );
    assert_eq!(
        read_token_amount(result.account(&scenario.keeper_collateral_token_account).unwrap()),
        bounty
    );
    assert_eq!(
        result.account(&scenario.leased_vault).map(|a| a.lamports).unwrap_or(0),
        0
    );
    assert_eq!(
        result.account(&scenario.collateral_vault).map(|a| a.lamports).unwrap_or(0),
        0
    );
}

#[test]
fn liquidate_rejects_healthy_position() {
    let (mut svm, scenario) = make_scenario();
    make_and_take(&mut svm, &scenario);

    // Price 1.0 → debt = 100 vs. 200 collateral → ratio 200% ≥ 120%.
    let price_update = Pubkey::new_unique();
    install_price_update(&mut svm, price_update, FEED_ID, 1, 0, DEFAULT_TIMESTAMP);

    let (instruction, accounts) = liquidate_call(&scenario, price_update);
    let result = svm.process_instruction(&instruction, &accounts);
    assert!(
        result.is_err(),
        "healthy liquidation must fail: {:?}",
        result.raw_result
    );
}

#[test]
fn liquidate_rejects_mismatched_price_feed() {
    let (mut svm, scenario) = make_scenario();
    make_and_take(&mut svm, &scenario);

    // Price that *would* trigger liquidation but with a foreign feed id —
    // the feed-pinning check must reject before the undercollateralisation
    // math runs.
    let wrong_feed_id = [0xCD; 32];
    let price_update = Pubkey::new_unique();
    install_price_update(&mut svm, price_update, wrong_feed_id, 4, 0, DEFAULT_TIMESTAMP);

    let (instruction, accounts) = liquidate_call(&scenario, price_update);
    let result = svm.process_instruction(&instruction, &accounts);
    assert!(
        result.is_err(),
        "liquidate must reject foreign price feeds: {:?}",
        result.raw_result
    );
}

#[test]
fn close_expired_reclaims_collateral_after_end_timestamp() {
    let (mut svm, scenario) = make_scenario();
    make_and_take(&mut svm, &scenario);

    // Jump past end_timestamp.
    svm.warp_to_timestamp(DEFAULT_TIMESTAMP + DURATION_SECONDS + 1);

    let (instruction, accounts) = close_expired_call(&scenario);
    let result = svm.process_instruction(&instruction, &accounts);
    assert!(result.is_ok(), "close_expired failed: {:?}", result.raw_result);

    // Full collateral forfeited to the lessor.
    assert_eq!(
        read_token_amount(result.account(&scenario.lessor_collateral_token_account).unwrap()),
        REQUIRED_COLLATERAL
    );
    // Lessor's leased balance is only what remained after the initial
    // escrow (the lessee kept the tokens on default).
    assert_eq!(
        read_token_amount(result.account(&scenario.lessor_leased_token_account).unwrap()),
        STARTING_BALANCE - LEASED_AMOUNT
    );
    assert_eq!(
        result.account(&scenario.leased_vault).map(|a| a.lamports).unwrap_or(0),
        0
    );
    assert_eq!(
        result.account(&scenario.collateral_vault).map(|a| a.lamports).unwrap_or(0),
        0
    );
}

#[test]
fn close_expired_cancels_listed_lease() {
    let (mut svm, scenario) = make_scenario();
    let (create_instruction, create_accounts) = create_lease_call(&scenario, 1);
    let create_result = svm.process_instruction(&create_instruction, &create_accounts);
    assert!(create_result.is_ok(), "create_lease failed: {:?}", create_result.raw_result);
    commit_state(
        &mut svm,
        &create_result,
        &[scenario.lease, scenario.leased_vault, scenario.collateral_vault, scenario.lessor_leased_token_account],
    );

    // Lessor bails while the lease is still `Listed` — allowed immediately.
    let (instruction, accounts) = close_expired_call(&scenario);
    let result = svm.process_instruction(&instruction, &accounts);
    assert!(result.is_ok(), "close_expired on Listed failed: {:?}", result.raw_result);

    // Lessor recovered the full leased amount. No collateral was posted.
    assert_eq!(
        read_token_amount(result.account(&scenario.lessor_leased_token_account).unwrap()),
        STARTING_BALANCE
    );
    assert_eq!(
        result.account(&scenario.leased_vault).map(|a| a.lamports).unwrap_or(0),
        0
    );
    assert_eq!(
        result.account(&scenario.collateral_vault).map(|a| a.lamports).unwrap_or(0),
        0
    );
}

#[test]
fn create_lease_rejects_same_mint_for_leased_and_collateral() {
    let (mut svm, scenario) = make_scenario();
    let (instruction, accounts) = create_lease_call_with_mints(&scenario, 42, scenario.leased_mint, scenario.leased_mint);
    let result = svm.process_instruction(&instruction, &accounts);
    assert!(
        result.is_err(),
        "create_lease must reject identical mints: {:?}",
        result.raw_result
    );
}
