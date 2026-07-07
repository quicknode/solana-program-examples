extern crate std;
use {
    crate::state::SECONDS_PER_DAY,
    quasar_lang::error::QuasarError,
    quasar_svm::{Account, Instruction, ProgramError, Pubkey, QuasarSvm},
    quasar_token_fundraiser_client::{
        CheckContributionsInstruction, ContributeInstruction, InitializeInstruction,
        QuasarTokenFundraiserError, RefundInstruction,
    },
    solana_program_pack::Pack,
    spl_token_interface::state::{Account as TokenAccount, AccountState, Mint},
    std::{vec, vec::Vec},
};

/// Fundraising target in minor units of the raised token.
const TARGET_AMOUNT: u64 = 10_000;
/// Fundraising window length in days.
const DURATION_DAYS: u16 = 30;
/// Arbitrary fixed unix timestamp the SVM clock is warped to before
/// initialize, so deadline math in tests is deterministic.
const START_TIME: i64 = 1_750_000_000;
/// First timestamp at which the fundraising window is closed.
const DEADLINE: i64 = START_TIME + DURATION_DAYS as i64 * SECONDS_PER_DAY;
/// Token balance each contributor's token account starts with.
const CONTRIBUTOR_STARTING_BALANCE: u64 = 100_000;
/// A contribution below the target, used by the refund-path tests.
const PARTIAL_CONTRIBUTION: u64 = 500;

fn setup() -> QuasarSvm {
    let elf = std::fs::read("target/deploy/quasar_token_fundraiser.so").unwrap();
    let mut svm = QuasarSvm::new()
        .with_program(&crate::ID, &elf)
        .with_token_program();
    svm.warp_to_timestamp(START_TIME);
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
            supply: 1_000_000_000,
            decimals: 9,
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

fn token_balance(svm: &QuasarSvm, address: &Pubkey) -> u64 {
    let account = svm.get_account(address).unwrap();
    TokenAccount::unpack(&account.data).unwrap().amount
}

fn find_fundraiser(maker: &Pubkey) -> (Pubkey, u8) {
    Pubkey::find_program_address(&[b"fundraiser", maker.as_ref()], &crate::ID)
}

fn find_contributor_account(fundraiser: &Pubkey, contributor: &Pubkey) -> (Pubkey, u8) {
    Pubkey::find_program_address(
        &[b"contributor", fundraiser.as_ref(), contributor.as_ref()],
        &crate::ID,
    )
}

/// Deserialized Fundraiser account state, parsed from the zero-copy layout:
/// [disc:1] [maker:32] [mint_to_raise:32] [vault:32] [amount_to_raise:8]
/// [current_amount:8] [time_started:8] [duration:2] [bump:1]
struct FundraiserState {
    maker: Pubkey,
    mint_to_raise: Pubkey,
    vault: Pubkey,
    amount_to_raise: u64,
    current_amount: u64,
    time_started: i64,
    duration: u16,
    bump: u8,
}

fn parse_fundraiser(data: &[u8]) -> FundraiserState {
    assert_eq!(data[0], 1, "Fundraiser discriminator");
    let mut cursor = Cursor {
        data,
        offset: 1usize,
    };
    FundraiserState {
        maker: Pubkey::new_from_array(cursor.take()),
        mint_to_raise: Pubkey::new_from_array(cursor.take()),
        vault: Pubkey::new_from_array(cursor.take()),
        amount_to_raise: u64::from_le_bytes(cursor.take()),
        current_amount: u64::from_le_bytes(cursor.take()),
        time_started: i64::from_le_bytes(cursor.take()),
        duration: u16::from_le_bytes(cursor.take()),
        bump: cursor.take::<1>()[0],
    }
}

/// Deserialized Contributor account state, parsed from the zero-copy layout:
/// [disc:1] [amount:8] [bump:1]
struct ContributorState {
    amount: u64,
    bump: u8,
}

fn parse_contributor(data: &[u8]) -> ContributorState {
    assert_eq!(data[0], 2, "Contributor discriminator");
    let mut cursor = Cursor {
        data,
        offset: 1usize,
    };
    ContributorState {
        amount: u64::from_le_bytes(cursor.take()),
        bump: cursor.take::<1>()[0],
    }
}

struct Cursor<'a> {
    data: &'a [u8],
    offset: usize,
}

impl Cursor<'_> {
    fn take<const N: usize>(&mut self) -> [u8; N] {
        let bytes: [u8; N] = self.data[self.offset..self.offset + N].try_into().unwrap();
        self.offset += N;
        bytes
    }
}

/// Addresses for one fundraiser plus one contributor, shared by every test.
struct Fixture {
    maker: Pubkey,
    mint: Pubkey,
    fundraiser: Pubkey,
    vault: Pubkey,
    contributor: Pubkey,
    contributor_ta: Pubkey,
    contributor_account: Pubkey,
}

fn fixture() -> Fixture {
    let maker = Pubkey::new_unique();
    let contributor = Pubkey::new_unique();
    let (fundraiser, _) = find_fundraiser(&maker);
    let (contributor_account, _) = find_contributor_account(&fundraiser, &contributor);
    Fixture {
        maker,
        mint: Pubkey::new_unique(),
        fundraiser,
        vault: Pubkey::new_unique(),
        contributor,
        contributor_ta: Pubkey::new_unique(),
        contributor_account,
    }
}

fn initialize_instruction(fixture: &Fixture, amount_to_raise: u64, duration: u16) -> Instruction {
    let mut instruction: Instruction = InitializeInstruction {
        maker: fixture.maker,
        mint_to_raise: fixture.mint,
        fundraiser: fixture.fundraiser,
        vault: fixture.vault,
        rent: quasar_svm::solana_sdk_ids::sysvar::rent::ID,
        token_program: quasar_svm::SPL_TOKEN_PROGRAM_ID,
        system_program: quasar_svm::system_program::ID,
        amount_to_raise,
        duration,
    }
    .into();
    // The vault is a fresh keypair account, so it must sign its own
    // system-program creation inside the init CPI.
    instruction.accounts[3].is_signer = true;
    instruction
}

fn initialize_accounts(fixture: &Fixture) -> Vec<Account> {
    vec![
        signer(fixture.maker),
        mint(fixture.mint, fixture.maker),
        empty(fixture.fundraiser),
        empty(fixture.vault),
    ]
}

/// Run initialize through the program and assert it succeeded.
fn initialize_fundraiser(svm: &mut QuasarSvm, fixture: &Fixture) {
    let result = svm.process_instruction(
        &initialize_instruction(fixture, TARGET_AMOUNT, DURATION_DAYS),
        &initialize_accounts(fixture),
    );
    result.assert_success();
}

fn contribute_instruction(fixture: &Fixture, amount: u64) -> Instruction {
    ContributeInstruction {
        contributor: fixture.contributor,
        maker: fixture.maker,
        fundraiser: fixture.fundraiser,
        contributor_account: fixture.contributor_account,
        contributor_ta: fixture.contributor_ta,
        vault: fixture.vault,
        mint_to_raise: fixture.mint,
        token_program: quasar_svm::SPL_TOKEN_PROGRAM_ID,
        system_program: quasar_svm::system_program::ID,
        amount,
    }
    .into()
}

/// Accounts a first-time contributor brings to contribute. The fundraiser
/// and vault already live in the SVM's database after initialize.
fn first_contribution_accounts(fixture: &Fixture) -> Vec<Account> {
    vec![
        signer(fixture.contributor),
        empty(fixture.contributor_account),
        token(
            fixture.contributor_ta,
            fixture.mint,
            fixture.contributor,
            CONTRIBUTOR_STARTING_BALANCE,
        ),
    ]
}

/// Run contribute through the program and assert it succeeded.
fn contribute(svm: &mut QuasarSvm, fixture: &Fixture, amount: u64) {
    let result = svm.process_instruction(
        &contribute_instruction(fixture, amount),
        &first_contribution_accounts(fixture),
    );
    result.assert_success();
}

fn refund_instruction(fixture: &Fixture) -> Instruction {
    RefundInstruction {
        contributor: fixture.contributor,
        maker: fixture.maker,
        fundraiser: fixture.fundraiser,
        contributor_account: fixture.contributor_account,
        contributor_ta: fixture.contributor_ta,
        vault: fixture.vault,
        mint_to_raise: fixture.mint,
        token_program: quasar_svm::SPL_TOKEN_PROGRAM_ID,
    }
    .into()
}

fn fundraiser_error(error: QuasarTokenFundraiserError) -> ProgramError {
    ProgramError::Custom(error as u32)
}

fn framework_error(error: QuasarError) -> ProgramError {
    ProgramError::Custom(error as u32)
}

#[test]
fn test_initialize_records_state_and_clock_time() {
    let mut svm = setup();
    let fixture = fixture();

    initialize_fundraiser(&mut svm, &fixture);

    let state = parse_fundraiser(&svm.get_account(&fixture.fundraiser).unwrap().data);
    assert_eq!(state.maker, fixture.maker);
    assert_eq!(state.mint_to_raise, fixture.mint);
    assert_eq!(state.vault, fixture.vault);
    assert_eq!(state.amount_to_raise, TARGET_AMOUNT);
    assert_eq!(state.current_amount, 0);
    assert_eq!(state.time_started, START_TIME);
    assert_eq!(state.duration, DURATION_DAYS);
    let (_, expected_bump) = find_fundraiser(&fixture.maker);
    assert_eq!(state.bump, expected_bump);

    assert_eq!(token_balance(&svm, &fixture.vault), 0);
}

#[test]
fn test_initialize_rejects_zero_amount() {
    let mut svm = setup();
    let fixture = fixture();

    let result = svm.process_instruction(
        &initialize_instruction(&fixture, 0, DURATION_DAYS),
        &initialize_accounts(&fixture),
    );
    result.assert_error(fundraiser_error(QuasarTokenFundraiserError::InvalidAmount));
}

#[test]
fn test_initialize_rejects_zero_duration() {
    let mut svm = setup();
    let fixture = fixture();

    let result = svm.process_instruction(
        &initialize_instruction(&fixture, TARGET_AMOUNT, 0),
        &initialize_accounts(&fixture),
    );
    result.assert_error(fundraiser_error(
        QuasarTokenFundraiserError::InvalidDuration,
    ));
}

#[test]
fn test_contribute_creates_contributor_account_and_moves_tokens() {
    let mut svm = setup();
    let fixture = fixture();
    initialize_fundraiser(&mut svm, &fixture);

    contribute(&mut svm, &fixture, PARTIAL_CONTRIBUTION);

    assert_eq!(token_balance(&svm, &fixture.vault), PARTIAL_CONTRIBUTION);
    assert_eq!(
        token_balance(&svm, &fixture.contributor_ta),
        CONTRIBUTOR_STARTING_BALANCE - PARTIAL_CONTRIBUTION
    );

    let fundraiser_state = parse_fundraiser(&svm.get_account(&fixture.fundraiser).unwrap().data);
    assert_eq!(fundraiser_state.current_amount, PARTIAL_CONTRIBUTION);

    let contributor_state =
        parse_contributor(&svm.get_account(&fixture.contributor_account).unwrap().data);
    assert_eq!(contributor_state.amount, PARTIAL_CONTRIBUTION);
    let (_, expected_bump) = find_contributor_account(&fixture.fundraiser, &fixture.contributor);
    assert_eq!(contributor_state.bump, expected_bump);
}

#[test]
fn test_contribute_accumulates_across_calls() {
    let mut svm = setup();
    let fixture = fixture();
    initialize_fundraiser(&mut svm, &fixture);
    contribute(&mut svm, &fixture, PARTIAL_CONTRIBUTION);

    // Second contribution reuses the contributor account created by the
    // first; everything already lives in the SVM database.
    let result =
        svm.process_instruction(&contribute_instruction(&fixture, PARTIAL_CONTRIBUTION), &[]);
    result.assert_success();

    let expected_total = PARTIAL_CONTRIBUTION * 2;
    assert_eq!(token_balance(&svm, &fixture.vault), expected_total);
    let contributor_state =
        parse_contributor(&svm.get_account(&fixture.contributor_account).unwrap().data);
    assert_eq!(contributor_state.amount, expected_total);
    let fundraiser_state = parse_fundraiser(&svm.get_account(&fixture.fundraiser).unwrap().data);
    assert_eq!(fundraiser_state.current_amount, expected_total);
}

#[test]
fn test_contribute_rejected_after_deadline() {
    let mut svm = setup();
    let fixture = fixture();
    initialize_fundraiser(&mut svm, &fixture);

    svm.warp_to_timestamp(DEADLINE);

    let result = svm.process_instruction(
        &contribute_instruction(&fixture, PARTIAL_CONTRIBUTION),
        &first_contribution_accounts(&fixture),
    );
    result.assert_error(fundraiser_error(
        QuasarTokenFundraiserError::FundraiserEnded,
    ));
}

#[test]
fn test_contribute_allowed_just_before_deadline() {
    let mut svm = setup();
    let fixture = fixture();
    initialize_fundraiser(&mut svm, &fixture);

    svm.warp_to_timestamp(DEADLINE - 1);

    contribute(&mut svm, &fixture, PARTIAL_CONTRIBUTION);
    assert_eq!(token_balance(&svm, &fixture.vault), PARTIAL_CONTRIBUTION);
}

#[test]
fn test_contribute_rejects_vault_not_bound_to_fundraiser() {
    let mut svm = setup();
    let fixture = fixture();
    initialize_fundraiser(&mut svm, &fixture);

    // The attacker tries to credit the fundraiser while depositing into a
    // decoy token account instead of the fundraiser's stored vault.
    let decoy_vault = Pubkey::new_unique();
    let mut accounts = first_contribution_accounts(&fixture);
    accounts.push(token(decoy_vault, fixture.mint, fixture.fundraiser, 0));

    let mut instruction = contribute_instruction(&fixture, PARTIAL_CONTRIBUTION);
    // Account index 5 is the vault (see ContributeInstruction ordering).
    instruction.accounts[5].pubkey = decoy_vault;

    let result = svm.process_instruction(&instruction, &accounts);
    result.assert_error(framework_error(QuasarError::HasOneMismatch));
}

#[test]
fn test_refund_returns_tokens_after_failed_fundraiser() {
    let mut svm = setup();
    let fixture = fixture();
    initialize_fundraiser(&mut svm, &fixture);
    contribute(&mut svm, &fixture, PARTIAL_CONTRIBUTION);

    svm.warp_to_timestamp(DEADLINE);

    let result = svm.process_instruction(&refund_instruction(&fixture), &[]);
    result.assert_success();

    assert_eq!(token_balance(&svm, &fixture.vault), 0);
    assert_eq!(
        token_balance(&svm, &fixture.contributor_ta),
        CONTRIBUTOR_STARTING_BALANCE
    );
    let fundraiser_state = parse_fundraiser(&svm.get_account(&fixture.fundraiser).unwrap().data);
    assert_eq!(fundraiser_state.current_amount, 0);

    // The contributor account was closed and its rent returned.
    let closed = svm.get_account(&fixture.contributor_account).unwrap();
    assert_eq!(closed.lamports, 0, "contributor account rent reclaimed");
}

#[test]
fn test_refund_rejected_before_deadline() {
    let mut svm = setup();
    let fixture = fixture();
    initialize_fundraiser(&mut svm, &fixture);
    contribute(&mut svm, &fixture, PARTIAL_CONTRIBUTION);

    svm.warp_to_timestamp(DEADLINE - 1);

    let result = svm.process_instruction(&refund_instruction(&fixture), &[]);
    result.assert_error(fundraiser_error(
        QuasarTokenFundraiserError::FundraiserNotEnded,
    ));
}

#[test]
fn test_refund_rejected_when_target_met() {
    let mut svm = setup();
    let fixture = fixture();
    initialize_fundraiser(&mut svm, &fixture);
    contribute(&mut svm, &fixture, TARGET_AMOUNT);

    svm.warp_to_timestamp(DEADLINE);

    let result = svm.process_instruction(&refund_instruction(&fixture), &[]);
    result.assert_error(fundraiser_error(QuasarTokenFundraiserError::TargetMet));
}

#[test]
fn test_refund_rejects_another_contributors_account() {
    let mut svm = setup();
    let fixture = fixture();
    initialize_fundraiser(&mut svm, &fixture);
    contribute(&mut svm, &fixture, PARTIAL_CONTRIBUTION);

    svm.warp_to_timestamp(DEADLINE);

    // The attacker signs as themselves but passes the victim's contributor
    // record and their own token account, trying to drain the vault.
    let attacker = Pubkey::new_unique();
    let attacker_ta = Pubkey::new_unique();
    let mut instruction = refund_instruction(&fixture);
    // Account indices follow RefundInstruction ordering:
    // 0 contributor (signer), 3 contributor_account, 4 contributor_ta.
    instruction.accounts[0].pubkey = attacker;
    instruction.accounts[4].pubkey = attacker_ta;

    let result = svm.process_instruction(
        &instruction,
        &[
            signer(attacker),
            token(attacker_ta, fixture.mint, attacker, 0),
        ],
    );
    // The contributor_account PDA check derives ["contributor", fundraiser,
    // attacker], which does not match the victim's record.
    result.assert_error(framework_error(QuasarError::InvalidPda));
    // The vault still holds the victim's contribution.
    assert_eq!(token_balance(&svm, &fixture.vault), PARTIAL_CONTRIBUTION);
}

#[test]
fn test_check_contributions_pays_maker_when_target_met() {
    let mut svm = setup();
    let fixture = fixture();
    initialize_fundraiser(&mut svm, &fixture);
    contribute(&mut svm, &fixture, TARGET_AMOUNT);

    let maker_ta = Pubkey::new_unique();
    let instruction: Instruction = CheckContributionsInstruction {
        maker: fixture.maker,
        fundraiser: fixture.fundraiser,
        vault: fixture.vault,
        maker_ta,
        mint_to_raise: fixture.mint,
        token_program: quasar_svm::SPL_TOKEN_PROGRAM_ID,
    }
    .into();

    let result =
        svm.process_instruction(&instruction, &[token(maker_ta, fixture.mint, fixture.maker, 0)]);
    result.assert_success();

    assert_eq!(token_balance(&svm, &maker_ta), TARGET_AMOUNT);
    // The vault and fundraiser accounts were closed.
    assert_eq!(svm.get_account(&fixture.vault).unwrap().lamports, 0);
    assert_eq!(svm.get_account(&fixture.fundraiser).unwrap().lamports, 0);
}

#[test]
fn test_check_contributions_rejected_below_target() {
    let mut svm = setup();
    let fixture = fixture();
    initialize_fundraiser(&mut svm, &fixture);
    contribute(&mut svm, &fixture, PARTIAL_CONTRIBUTION);

    let maker_ta = Pubkey::new_unique();
    let instruction: Instruction = CheckContributionsInstruction {
        maker: fixture.maker,
        fundraiser: fixture.fundraiser,
        vault: fixture.vault,
        maker_ta,
        mint_to_raise: fixture.mint,
        token_program: quasar_svm::SPL_TOKEN_PROGRAM_ID,
    }
    .into();

    let result =
        svm.process_instruction(&instruction, &[token(maker_ta, fixture.mint, fixture.maker, 0)]);
    result.assert_error(fundraiser_error(QuasarTokenFundraiserError::TargetNotMet));
}
