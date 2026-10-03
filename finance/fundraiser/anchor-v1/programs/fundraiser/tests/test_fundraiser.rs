use {
    anchor_lang::{
        solana_program::{clock::Clock, instruction::Instruction, pubkey::Pubkey, system_program},
        InstructionData, ToAccountMetas,
    },
    borsh::BorshDeserialize,
    fundraiser::{FundraiserError, SECONDS_TO_DAYS},
    litesvm::LiteSVM,
    solana_keypair::Keypair,
    solana_kite::{
        create_associated_token_account, create_token_mint, create_wallet,
        get_token_account_balance, mint_tokens_to_token_account,
        send_transaction_from_instructions,
    },
    solana_signer::Signer,
};

const MINT_DECIMALS: u8 = 6;
/// One major unit of the test mint in minor units (10^MINT_DECIMALS).
const ONE_TOKEN: u64 = 1_000_000;
/// Comfortably above the program's 3-major-unit minimum target.
const AMOUNT_TO_RAISE: u64 = 30 * ONE_TOKEN;
/// Three unequal contributions that together reach the target exactly.
const CONTRIBUTIONS_REACHING_TARGET: [u64; 3] = [12 * ONE_TOKEN, 10 * ONE_TOKEN, 8 * ONE_TOKEN];
/// A contribution well short of the target on its own.
const CONTRIBUTION: u64 = 4 * ONE_TOKEN;
const DURATION_DAYS: u16 = 7;
const CONTRIBUTOR_STARTING_BALANCE: u64 = 20 * ONE_TOKEN;
/// LiteSVM's fee for a transaction with one signature.
const TRANSACTION_FEE: u64 = 5_000;

fn token_program_id() -> Pubkey {
    "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA"
        .parse()
        .unwrap()
}

fn ata_program_id() -> Pubkey {
    "ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL"
        .parse()
        .unwrap()
}

fn derive_ata(wallet: &Pubkey, mint: &Pubkey) -> Pubkey {
    let (ata, _bump) = Pubkey::find_program_address(
        &[wallet.as_ref(), token_program_id().as_ref(), mint.as_ref()],
        &ata_program_id(),
    );
    ata
}

/// Mirror of the onchain Fundraiser struct for borsh-decoding account data
/// in tests. Pubkeys are read as raw 32-byte arrays.
#[derive(BorshDeserialize)]
struct FundraiserState {
    _maker: [u8; 32],
    _mint_to_raise: [u8; 32],
    amount_to_raise: u64,
    current_amount: u64,
    _time_started: i64,
    duration: u16,
    claimed: bool,
    open_contributions: u32,
    _bump: u8,
}

/// Mirror of the onchain Contribution struct.
#[derive(BorshDeserialize)]
struct ContributionState {
    amount: u64,
    _bump: u8,
}

const ANCHOR_DISCRIMINATOR_LENGTH: usize = 8;

fn read_fundraiser_state(svm: &LiteSVM, fundraiser_pda: &Pubkey) -> FundraiserState {
    let account = svm.get_account(fundraiser_pda).unwrap();
    FundraiserState::try_from_slice(&account.data[ANCHOR_DISCRIMINATOR_LENGTH..]).unwrap()
}

fn read_contribution_state(svm: &LiteSVM, contribution_pda: &Pubkey) -> ContributionState {
    let account = svm.get_account(contribution_pda).unwrap();
    ContributionState::try_from_slice(&account.data[ANCHOR_DISCRIMINATOR_LENGTH..]).unwrap()
}

/// Moves the LiteSVM clock forward by the given number of days.
fn warp_days_forward(svm: &mut LiteSVM, days: i64) {
    let mut clock: Clock = svm.get_sysvar();
    clock.unix_timestamp += days * SECONDS_TO_DAYS;
    svm.set_sysvar(&clock);
}

struct FundraiserSetup {
    svm: LiteSVM,
    program_id: Pubkey,
    payer: Keypair,
    maker: Keypair,
    mint: Pubkey,
    fundraiser_pda: Pubkey,
    vault: Pubkey,
}

fn full_setup() -> FundraiserSetup {
    let program_id = fundraiser::id();
    let mut svm = LiteSVM::new();

    let program_bytes = include_bytes!("../../../target/deploy/fundraiser.so");
    svm.add_program(program_id, program_bytes).unwrap();

    let payer = create_wallet(&mut svm, 100_000_000_000).unwrap();
    let maker = create_wallet(&mut svm, 10_000_000_000).unwrap();

    // The payer is the mint authority.
    let mint = create_token_mint(&mut svm, &payer, MINT_DECIMALS, None).unwrap();

    let (fundraiser_pda, _bump) =
        Pubkey::find_program_address(&[b"fundraiser", maker.pubkey().as_ref()], &program_id);

    // The vault is the ATA of the fundraiser PDA for the mint.
    let vault = derive_ata(&fundraiser_pda, &mint);

    FundraiserSetup {
        svm,
        program_id,
        payer,
        maker,
        mint,
        fundraiser_pda,
        vault,
    }
}

fn initialize_fundraiser(setup: &mut FundraiserSetup, amount: u64, duration: u16) {
    let initialize_instruction = Instruction::new_with_bytes(
        setup.program_id,
        &fundraiser::instruction::InitializeFundraiser { amount, duration }.data(),
        fundraiser::accounts::InitializeFundraiserAccountConstraints {
            maker: setup.maker.pubkey(),
            mint_to_raise: setup.mint,
            fundraiser: setup.fundraiser_pda,
            vault: setup.vault,
            system_program: system_program::id(),
            token_program: token_program_id(),
            associated_token_program: ata_program_id(),
        }
        .to_account_metas(None),
    );
    send_transaction_from_instructions(
        &mut setup.svm,
        vec![initialize_instruction],
        &[&setup.maker],
        &setup.maker.pubkey(),
    )
    .unwrap();
}

/// Creates a contributor wallet with a funded ATA and returns
/// (contributor keypair, contributor ATA, contribution PDA).
fn create_funded_contributor(setup: &mut FundraiserSetup) -> (Keypair, Pubkey, Pubkey) {
    let contributor = create_wallet(&mut setup.svm, 10_000_000_000).unwrap();

    let contributor_ata = create_associated_token_account(
        &mut setup.svm,
        &contributor.pubkey(),
        &setup.mint,
        &setup.payer,
    )
    .unwrap();

    mint_tokens_to_token_account(
        &mut setup.svm,
        &setup.mint,
        &contributor_ata,
        CONTRIBUTOR_STARTING_BALANCE,
        &setup.payer,
    )
    .unwrap();

    let (contribution_pda, _bump) = Pubkey::find_program_address(
        &[
            b"contribution",
            setup.fundraiser_pda.as_ref(),
            contributor.pubkey().as_ref(),
        ],
        &setup.program_id,
    );

    (contributor, contributor_ata, contribution_pda)
}

fn build_contribute_instruction(
    setup: &FundraiserSetup,
    contributor: &Pubkey,
    contributor_ata: &Pubkey,
    contribution_pda: &Pubkey,
    amount: u64,
) -> Instruction {
    Instruction::new_with_bytes(
        setup.program_id,
        &fundraiser::instruction::Contribute { amount }.data(),
        fundraiser::accounts::ContributeAccountConstraints {
            contributor: *contributor,
            mint_to_raise: setup.mint,
            fundraiser: setup.fundraiser_pda,
            contribution: *contribution_pda,
            contributor_ata: *contributor_ata,
            vault: setup.vault,
            token_program: token_program_id(),
            system_program: system_program::id(),
        }
        .to_account_metas(None),
    )
}

fn build_refund_instruction(
    setup: &FundraiserSetup,
    contributor: &Pubkey,
    contributor_ata: &Pubkey,
    contribution_pda: &Pubkey,
) -> Instruction {
    Instruction::new_with_bytes(
        setup.program_id,
        &fundraiser::instruction::Refund {}.data(),
        fundraiser::accounts::RefundAccountConstraints {
            contributor: *contributor,
            maker: setup.maker.pubkey(),
            mint_to_raise: setup.mint,
            fundraiser: setup.fundraiser_pda,
            contribution: *contribution_pda,
            contributor_ata: *contributor_ata,
            vault: setup.vault,
            token_program: token_program_id(),
            system_program: system_program::id(),
        }
        .to_account_metas(None),
    )
}

fn build_check_contributions_instruction(
    setup: &FundraiserSetup,
    maker_ata: &Pubkey,
) -> Instruction {
    Instruction::new_with_bytes(
        setup.program_id,
        &fundraiser::instruction::CheckContributions {}.data(),
        fundraiser::accounts::CheckContributionsAccountConstraints {
            maker: setup.maker.pubkey(),
            mint_to_raise: setup.mint,
            fundraiser: setup.fundraiser_pda,
            vault: setup.vault,
            maker_ata: *maker_ata,
            token_program: token_program_id(),
            system_program: system_program::id(),
            associated_token_program: ata_program_id(),
        }
        .to_account_metas(None),
    )
}

fn build_close_contribution_instruction(
    setup: &FundraiserSetup,
    contributor: &Pubkey,
    contribution_pda: &Pubkey,
) -> Instruction {
    Instruction::new_with_bytes(
        setup.program_id,
        &fundraiser::instruction::CloseContribution {}.data(),
        fundraiser::accounts::CloseContributionAccountConstraints {
            contributor: *contributor,
            fundraiser: setup.fundraiser_pda,
            contribution: *contribution_pda,
        }
        .to_account_metas(None),
    )
}

fn build_close_fundraiser_instruction(setup: &FundraiserSetup, maker_ata: &Pubkey) -> Instruction {
    Instruction::new_with_bytes(
        setup.program_id,
        &fundraiser::instruction::CloseFundraiser {}.data(),
        fundraiser::accounts::CloseFundraiserAccountConstraints {
            maker: setup.maker.pubkey(),
            mint_to_raise: setup.mint,
            fundraiser: setup.fundraiser_pda,
            vault: setup.vault,
            maker_ata: *maker_ata,
            token_program: token_program_id(),
            system_program: system_program::id(),
            associated_token_program: ata_program_id(),
        }
        .to_account_metas(None),
    )
}

struct FundedContributor {
    keypair: Keypair,
    ata: Pubkey,
    contribution_pda: Pubkey,
}

/// Sends `contribute` for the given contributor and amount, signed by the
/// contributor.
fn contribute(
    setup: &mut FundraiserSetup,
    contributor: &FundedContributor,
    amount: u64,
) -> Result<(), String> {
    let contribute_instruction = build_contribute_instruction(
        setup,
        &contributor.keypair.pubkey(),
        &contributor.ata,
        &contributor.contribution_pda,
        amount,
    );
    send_transaction_from_instructions(
        &mut setup.svm,
        vec![contribute_instruction],
        &[&contributor.keypair],
        &contributor.keypair.pubkey(),
    )
    .map(|_| ())
    .map_err(|error| format!("{error:?}"))
}

fn new_contributor(setup: &mut FundraiserSetup) -> FundedContributor {
    let (keypair, ata, contribution_pda) = create_funded_contributor(setup);
    FundedContributor {
        keypair,
        ata,
        contribution_pda,
    }
}

/// Creates three contributors whose contributions reach the target exactly.
fn fund_to_target(setup: &mut FundraiserSetup) -> Vec<FundedContributor> {
    CONTRIBUTIONS_REACHING_TARGET
        .iter()
        .map(|amount| {
            let contributor = new_contributor(setup);
            contribute(setup, &contributor, *amount).unwrap();
            contributor
        })
        .collect()
}

/// Sends `refund` for `contributor`, paid for by `fee_payer`, who need not be the contributor.
fn refund(
    setup: &mut FundraiserSetup,
    fee_payer: &Keypair,
    contributor: &FundedContributor,
) -> Result<(), String> {
    let refund_instruction = build_refund_instruction(
        setup,
        &contributor.keypair.pubkey(),
        &contributor.ata,
        &contributor.contribution_pda,
    );
    send_transaction_from_instructions(
        &mut setup.svm,
        vec![refund_instruction],
        &[fee_payer],
        &fee_payer.pubkey(),
    )
    .map(|_| ())
    .map_err(|error| format!("{error:?}"))
}

/// Sends `check_contributions`, signed by the maker.
fn claim(setup: &mut FundraiserSetup) -> Result<Pubkey, String> {
    let maker_ata = derive_ata(&setup.maker.pubkey(), &setup.mint);
    let check_instruction = build_check_contributions_instruction(setup, &maker_ata);
    let maker = setup.maker.insecure_clone();
    send_transaction_from_instructions(
        &mut setup.svm,
        vec![check_instruction],
        &[&maker],
        &maker.pubkey(),
    )
    .map(|_| maker_ata)
    .map_err(|error| format!("{error:?}"))
}

/// Sends `close_contribution` for `contributor`, signed and paid for by
/// `fee_payer`.
fn close_contribution(
    setup: &mut FundraiserSetup,
    fee_payer: &Keypair,
    contributor: &FundedContributor,
) -> Result<(), String> {
    let close_instruction = build_close_contribution_instruction(
        setup,
        &contributor.keypair.pubkey(),
        &contributor.contribution_pda,
    );
    send_transaction_from_instructions(
        &mut setup.svm,
        vec![close_instruction],
        &[fee_payer],
        &fee_payer.pubkey(),
    )
    .map(|_| ())
    .map_err(|error| format!("{error:?}"))
}

/// Sends `close_fundraiser`, signed by the maker.
fn close_fundraiser(setup: &mut FundraiserSetup) -> Result<(), String> {
    let maker_ata = derive_ata(&setup.maker.pubkey(), &setup.mint);
    let close_instruction = build_close_fundraiser_instruction(setup, &maker_ata);
    let maker = setup.maker.insecure_clone();
    send_transaction_from_instructions(
        &mut setup.svm,
        vec![close_instruction],
        &[&maker],
        &maker.pubkey(),
    )
    .map(|_| ())
    .map_err(|error| format!("{error:?}"))
}

fn lamports(setup: &FundraiserSetup, address: &Pubkey) -> u64 {
    setup
        .svm
        .get_account(address)
        .map_or(0, |account| account.lamports)
}

/// Anchor numbers a program's `#[error_code]` variants from 6000.
const ANCHOR_ERROR_CODE_OFFSET: u32 = 6000;

/// Asserts that a transaction failed with the given program error.
fn assert_error<T: std::fmt::Debug>(result: Result<T, String>, expected_error: FundraiserError) {
    let error = result.expect_err("transaction should have failed");
    let expected_code = format!(
        "Custom({})",
        ANCHOR_ERROR_CODE_OFFSET + expected_error as u32
    );
    assert!(
        error.contains(&expected_code),
        "expected {expected_code}, got: {error}"
    );
}

#[test]
fn test_initialize_fundraiser() {
    let mut setup = full_setup();

    initialize_fundraiser(&mut setup, AMOUNT_TO_RAISE, DURATION_DAYS);

    let fundraiser_state = read_fundraiser_state(&setup.svm, &setup.fundraiser_pda);
    assert_eq!(fundraiser_state.amount_to_raise, AMOUNT_TO_RAISE);
    assert_eq!(fundraiser_state.current_amount, 0);
    assert_eq!(fundraiser_state.duration, DURATION_DAYS);
    assert!(!fundraiser_state.claimed);
    assert_eq!(fundraiser_state.open_contributions, 0);

    assert_eq!(
        get_token_account_balance(&setup.svm, &setup.vault).unwrap(),
        0
    );
}

#[test]
fn test_initialize_below_minimum_target_fails() {
    let mut setup = full_setup();

    // 3 major units is the minimum; one minor unit below it must fail.
    let below_minimum_target = 3 * ONE_TOKEN - 1;
    let initialize_instruction = Instruction::new_with_bytes(
        setup.program_id,
        &fundraiser::instruction::InitializeFundraiser {
            amount: below_minimum_target,
            duration: DURATION_DAYS,
        }
        .data(),
        fundraiser::accounts::InitializeFundraiserAccountConstraints {
            maker: setup.maker.pubkey(),
            mint_to_raise: setup.mint,
            fundraiser: setup.fundraiser_pda,
            vault: setup.vault,
            system_program: system_program::id(),
            token_program: token_program_id(),
            associated_token_program: ata_program_id(),
        }
        .to_account_metas(None),
    );
    let result = send_transaction_from_instructions(
        &mut setup.svm,
        vec![initialize_instruction],
        &[&setup.maker],
        &setup.maker.pubkey(),
    )
    .map_err(|error| format!("{error:?}"));
    assert_error(result, FundraiserError::InvalidAmount);
    assert!(
        setup.svm.get_account(&setup.fundraiser_pda).is_none(),
        "Fundraiser account must not exist after a failed initialize"
    );
}

#[test]
fn test_contribute_inside_window_succeeds() {
    let mut setup = full_setup();
    initialize_fundraiser(&mut setup, AMOUNT_TO_RAISE, DURATION_DAYS);
    let contributor = new_contributor(&mut setup);

    // One day in: well inside the 7-day window.
    warp_days_forward(&mut setup.svm, 1);
    contribute(&mut setup, &contributor, CONTRIBUTION).unwrap();

    assert_eq!(
        get_token_account_balance(&setup.svm, &setup.vault).unwrap(),
        CONTRIBUTION
    );
    assert_eq!(
        get_token_account_balance(&setup.svm, &contributor.ata).unwrap(),
        CONTRIBUTOR_STARTING_BALANCE - CONTRIBUTION
    );

    let fundraiser_state = read_fundraiser_state(&setup.svm, &setup.fundraiser_pda);
    assert_eq!(fundraiser_state.current_amount, CONTRIBUTION);
    assert_eq!(fundraiser_state.open_contributions, 1);

    let contribution_state = read_contribution_state(&setup.svm, &contributor.contribution_pda);
    assert_eq!(contribution_state.amount, CONTRIBUTION);
}

#[test]
fn test_contributions_accumulate_in_one_contribution() {
    let mut setup = full_setup();
    initialize_fundraiser(&mut setup, AMOUNT_TO_RAISE, DURATION_DAYS);
    let contributor = new_contributor(&mut setup);

    let first_contribution = 5 * ONE_TOKEN;
    let second_contribution = 3 * ONE_TOKEN;
    contribute(&mut setup, &contributor, first_contribution).unwrap();
    warp_days_forward(&mut setup.svm, 1);
    contribute(&mut setup, &contributor, second_contribution).unwrap();

    let contribution_state = read_contribution_state(&setup.svm, &contributor.contribution_pda);
    assert_eq!(
        contribution_state.amount,
        first_contribution + second_contribution
    );

    // A second contribution adds to the existing account rather than
    // creating another, so the fundraiser still counts one.
    let fundraiser_state = read_fundraiser_state(&setup.svm, &setup.fundraiser_pda);
    assert_eq!(
        fundraiser_state.current_amount,
        first_contribution + second_contribution
    );
    assert_eq!(fundraiser_state.open_contributions, 1);
}

#[test]
fn test_contribute_after_deadline_fails() {
    let mut setup = full_setup();
    initialize_fundraiser(&mut setup, AMOUNT_TO_RAISE, DURATION_DAYS);
    let contributor = new_contributor(&mut setup);

    // The deadline falls exactly DURATION_DAYS after the start.
    warp_days_forward(&mut setup.svm, DURATION_DAYS as i64);

    assert_error(
        contribute(&mut setup, &contributor, ONE_TOKEN),
        FundraiserError::FundraiserEnded,
    );
    assert_eq!(
        get_token_account_balance(&setup.svm, &setup.vault).unwrap(),
        0
    );
    assert_eq!(
        get_token_account_balance(&setup.svm, &contributor.ata).unwrap(),
        CONTRIBUTOR_STARTING_BALANCE
    );
}

#[test]
fn test_contribute_below_one_major_unit_fails() {
    let mut setup = full_setup();
    initialize_fundraiser(&mut setup, AMOUNT_TO_RAISE, DURATION_DAYS);
    let contributor = new_contributor(&mut setup);

    assert_error(
        contribute(&mut setup, &contributor, ONE_TOKEN - 1),
        FundraiserError::ContributionTooSmall,
    );
    assert_eq!(
        get_token_account_balance(&setup.svm, &setup.vault).unwrap(),
        0
    );
}

#[test]
fn test_refund_before_deadline_fails() {
    let mut setup = full_setup();
    initialize_fundraiser(&mut setup, AMOUNT_TO_RAISE, DURATION_DAYS);
    let contributor = new_contributor(&mut setup);
    contribute(&mut setup, &contributor, ONE_TOKEN).unwrap();

    // One day short of the deadline.
    warp_days_forward(&mut setup.svm, DURATION_DAYS as i64 - 1);

    let fee_payer = contributor.keypair.insecure_clone();
    assert_error(
        refund(&mut setup, &fee_payer, &contributor),
        FundraiserError::FundraiserNotEnded,
    );
    assert_eq!(
        get_token_account_balance(&setup.svm, &setup.vault).unwrap(),
        ONE_TOKEN
    );
    let fundraiser_state = read_fundraiser_state(&setup.svm, &setup.fundraiser_pda);
    assert_eq!(fundraiser_state.current_amount, ONE_TOKEN);
}

#[test]
fn test_refund_after_deadline_target_not_met_succeeds() {
    let mut setup = full_setup();
    initialize_fundraiser(&mut setup, AMOUNT_TO_RAISE, DURATION_DAYS);
    let contributor = new_contributor(&mut setup);
    contribute(&mut setup, &contributor, CONTRIBUTION).unwrap();

    // Past the deadline, target not met: refund must succeed.
    warp_days_forward(&mut setup.svm, DURATION_DAYS as i64);

    let fee_payer = contributor.keypair.insecure_clone();
    refund(&mut setup, &fee_payer, &contributor).unwrap();

    assert_eq!(
        get_token_account_balance(&setup.svm, &setup.vault).unwrap(),
        0
    );
    assert_eq!(
        get_token_account_balance(&setup.svm, &contributor.ata).unwrap(),
        CONTRIBUTOR_STARTING_BALANCE
    );

    let fundraiser_state = read_fundraiser_state(&setup.svm, &setup.fundraiser_pda);
    assert_eq!(fundraiser_state.current_amount, 0);
    assert_eq!(fundraiser_state.open_contributions, 0);

    assert!(
        setup
            .svm
            .get_account(&contributor.contribution_pda)
            .is_none(),
        "Contribution account must be closed after refund"
    );
}

#[test]
fn test_anyone_can_refund_a_contributor() {
    let mut setup = full_setup();
    initialize_fundraiser(&mut setup, AMOUNT_TO_RAISE, DURATION_DAYS);
    let contributor = new_contributor(&mut setup);
    contribute(&mut setup, &contributor, CONTRIBUTION).unwrap();
    warp_days_forward(&mut setup.svm, DURATION_DAYS as i64);

    // The maker sends the refund. The tokens and the rent still go to the
    // contributor, who signs nothing.
    let rent = lamports(&setup, &contributor.contribution_pda);
    let contributor_lamports_before = lamports(&setup, &contributor.keypair.pubkey());
    let maker = setup.maker.insecure_clone();
    refund(&mut setup, &maker, &contributor).unwrap();

    assert_eq!(
        get_token_account_balance(&setup.svm, &contributor.ata).unwrap(),
        CONTRIBUTOR_STARTING_BALANCE
    );
    assert_eq!(
        lamports(&setup, &contributor.keypair.pubkey()),
        contributor_lamports_before + rent
    );
}

#[test]
fn test_refund_when_target_met_fails() {
    let mut setup = full_setup();
    initialize_fundraiser(&mut setup, AMOUNT_TO_RAISE, DURATION_DAYS);
    let contributors = fund_to_target(&mut setup);

    warp_days_forward(&mut setup.svm, DURATION_DAYS as i64);

    let fee_payer = contributors[0].keypair.insecure_clone();
    assert_error(
        refund(&mut setup, &fee_payer, &contributors[0]),
        FundraiserError::TargetMet,
    );
    assert_eq!(
        get_token_account_balance(&setup.svm, &setup.vault).unwrap(),
        AMOUNT_TO_RAISE
    );
}

#[test]
fn test_check_contributions_success_pays_maker_and_marks_claimed() {
    let mut setup = full_setup();
    initialize_fundraiser(&mut setup, AMOUNT_TO_RAISE, DURATION_DAYS);
    fund_to_target(&mut setup);

    let maker_ata = claim(&mut setup).unwrap();

    assert_eq!(
        get_token_account_balance(&setup.svm, &maker_ata).unwrap(),
        AMOUNT_TO_RAISE
    );
    assert_eq!(
        get_token_account_balance(&setup.svm, &setup.vault).unwrap(),
        0
    );

    // The fundraiser stays open, marked claimed, until every contributor
    // account written for it is closed.
    let fundraiser_state = read_fundraiser_state(&setup.svm, &setup.fundraiser_pda);
    assert!(fundraiser_state.claimed);
    assert_eq!(
        fundraiser_state.open_contributions,
        CONTRIBUTIONS_REACHING_TARGET.len() as u32
    );
}

#[test]
fn test_second_claim_fails() {
    let mut setup = full_setup();
    initialize_fundraiser(&mut setup, AMOUNT_TO_RAISE, DURATION_DAYS);
    fund_to_target(&mut setup);
    let maker_ata = claim(&mut setup).unwrap();

    // A donation to the vault after the claim must not make a second claim
    // possible.
    mint_tokens_to_token_account(
        &mut setup.svm,
        &setup.mint,
        &setup.vault,
        ONE_TOKEN,
        &setup.payer,
    )
    .unwrap();
    setup.svm.expire_blockhash();

    assert_error(claim(&mut setup), FundraiserError::FundraiserClaimed);
    assert_eq!(
        get_token_account_balance(&setup.svm, &maker_ata).unwrap(),
        AMOUNT_TO_RAISE
    );
}

#[test]
fn test_contribute_after_claim_fails() {
    let mut setup = full_setup();
    initialize_fundraiser(&mut setup, AMOUNT_TO_RAISE, DURATION_DAYS);
    fund_to_target(&mut setup);
    claim(&mut setup).unwrap();

    // The deadline is still days away, but the vault has been paid out.
    warp_days_forward(&mut setup.svm, 1);
    let late_contributor = new_contributor(&mut setup);
    assert_error(
        contribute(&mut setup, &late_contributor, CONTRIBUTION),
        FundraiserError::FundraiserClaimed,
    );
    assert_eq!(
        get_token_account_balance(&setup.svm, &late_contributor.ata).unwrap(),
        CONTRIBUTOR_STARTING_BALANCE
    );
}

#[test]
fn test_check_contributions_ignores_direct_vault_donations() {
    let mut setup = full_setup();
    initialize_fundraiser(&mut setup, AMOUNT_TO_RAISE, DURATION_DAYS);

    // Mint the full target straight into the vault, bypassing contribute.
    // The state-tracked current_amount stays 0, so the claim must fail.
    mint_tokens_to_token_account(
        &mut setup.svm,
        &setup.mint,
        &setup.vault,
        AMOUNT_TO_RAISE,
        &setup.payer,
    )
    .unwrap();

    assert_error(claim(&mut setup), FundraiserError::TargetNotMet);
    let fundraiser_state = read_fundraiser_state(&setup.svm, &setup.fundraiser_pda);
    assert!(!fundraiser_state.claimed);
}

#[test]
fn test_close_contribution_after_claim_returns_rent() {
    let mut setup = full_setup();
    initialize_fundraiser(&mut setup, AMOUNT_TO_RAISE, DURATION_DAYS);
    let contributors = fund_to_target(&mut setup);
    claim(&mut setup).unwrap();

    let contributor = &contributors[0];
    let rent = lamports(&setup, &contributor.contribution_pda);
    let lamports_before = lamports(&setup, &contributor.keypair.pubkey());

    let fee_payer = contributor.keypair.insecure_clone();
    close_contribution(&mut setup, &fee_payer, contributor).unwrap();

    assert!(
        setup
            .svm
            .get_account(&contributor.contribution_pda)
            .is_none(),
        "Contribution account must be closed"
    );
    // The contributor paid the transaction fee out of the same balance, so
    // the rent came back less that fee.
    assert_eq!(
        lamports(&setup, &contributor.keypair.pubkey()),
        lamports_before + rent - TRANSACTION_FEE
    );
    let fundraiser_state = read_fundraiser_state(&setup.svm, &setup.fundraiser_pda);
    assert_eq!(
        fundraiser_state.open_contributions,
        CONTRIBUTIONS_REACHING_TARGET.len() as u32 - 1
    );
}

#[test]
fn test_anyone_can_close_contribution_accounts_after_claim() {
    let mut setup = full_setup();
    initialize_fundraiser(&mut setup, AMOUNT_TO_RAISE, DURATION_DAYS);
    let contributors = fund_to_target(&mut setup);
    claim(&mut setup).unwrap();

    // The maker closes every contribution account; each rent deposit goes to
    // its contributor, who signs nothing.
    let maker = setup.maker.insecure_clone();
    for contributor in &contributors {
        let rent = lamports(&setup, &contributor.contribution_pda);
        let lamports_before = lamports(&setup, &contributor.keypair.pubkey());
        close_contribution(&mut setup, &maker, contributor).unwrap();
        assert_eq!(
            lamports(&setup, &contributor.keypair.pubkey()),
            lamports_before + rent
        );
    }

    let fundraiser_state = read_fundraiser_state(&setup.svm, &setup.fundraiser_pda);
    assert_eq!(fundraiser_state.open_contributions, 0);
}

#[test]
fn test_close_contribution_before_claim_fails() {
    let mut setup = full_setup();
    initialize_fundraiser(&mut setup, AMOUNT_TO_RAISE, DURATION_DAYS);
    let contributor = new_contributor(&mut setup);
    contribute(&mut setup, &contributor, CONTRIBUTION).unwrap();

    // The fundraiser is unclaimed, so the contribution can still be
    // refunded: closing the account now would erase what the vault owes.
    let fee_payer = contributor.keypair.insecure_clone();
    assert_error(
        close_contribution(&mut setup, &fee_payer, &contributor),
        FundraiserError::FundraiserNotClaimed,
    );
    let contribution_state = read_contribution_state(&setup.svm, &contributor.contribution_pda);
    assert_eq!(contribution_state.amount, CONTRIBUTION);
}

#[test]
fn test_close_fundraiser_after_failed_raise_allows_a_new_raise() {
    let mut setup = full_setup();
    initialize_fundraiser(&mut setup, AMOUNT_TO_RAISE, DURATION_DAYS);
    let contributor = new_contributor(&mut setup);
    contribute(&mut setup, &contributor, CONTRIBUTION).unwrap();

    // The raise fails; the contributor takes their refund.
    warp_days_forward(&mut setup.svm, DURATION_DAYS as i64);
    let fee_payer = contributor.keypair.insecure_clone();
    refund(&mut setup, &fee_payer, &contributor).unwrap();

    close_fundraiser(&mut setup).unwrap();
    assert!(
        setup.svm.get_account(&setup.vault).is_none(),
        "Vault token account must be closed with the fundraiser"
    );
    assert!(
        setup.svm.get_account(&setup.fundraiser_pda).is_none(),
        "Fundraiser account must be closed after a failed raise is retired"
    );

    // The same maker can now open a fresh fundraiser at the same PDA. The
    // retry would otherwise be byte-identical to the first initialize (same
    // accounts, data, and blockhash), which LiteSVM rejects as already
    // processed.
    setup.svm.expire_blockhash();
    initialize_fundraiser(&mut setup, AMOUNT_TO_RAISE, DURATION_DAYS);
    let fundraiser_state = read_fundraiser_state(&setup.svm, &setup.fundraiser_pda);
    assert_eq!(fundraiser_state.current_amount, 0);
    assert_eq!(fundraiser_state.amount_to_raise, AMOUNT_TO_RAISE);
    assert_eq!(
        get_token_account_balance(&setup.svm, &setup.vault).unwrap(),
        0
    );
}

#[test]
fn test_close_fundraiser_before_deadline_fails() {
    let mut setup = full_setup();
    initialize_fundraiser(&mut setup, AMOUNT_TO_RAISE, DURATION_DAYS);

    // One day short of the deadline.
    warp_days_forward(&mut setup.svm, DURATION_DAYS as i64 - 1);

    assert_error(
        close_fundraiser(&mut setup),
        FundraiserError::FundraiserNotEnded,
    );
    assert!(
        setup.svm.get_account(&setup.fundraiser_pda).is_some(),
        "Fundraiser account must stay open after a failed close"
    );
}

#[test]
fn test_close_fundraiser_with_unrefunded_contributions_fails() {
    let mut setup = full_setup();
    initialize_fundraiser(&mut setup, AMOUNT_TO_RAISE, DURATION_DAYS);
    let contributor = new_contributor(&mut setup);
    contribute(&mut setup, &contributor, CONTRIBUTION).unwrap();

    // Past the deadline but the contribution has not been refunded, so
    // closing would strand it in the vault.
    warp_days_forward(&mut setup.svm, DURATION_DAYS as i64);

    assert_error(
        close_fundraiser(&mut setup),
        FundraiserError::RefundsOutstanding,
    );
    assert_eq!(
        get_token_account_balance(&setup.svm, &setup.vault).unwrap(),
        CONTRIBUTION
    );
}

#[test]
fn test_close_fundraiser_when_target_met_but_unclaimed_fails() {
    let mut setup = full_setup();
    initialize_fundraiser(&mut setup, AMOUNT_TO_RAISE, DURATION_DAYS);
    fund_to_target(&mut setup);

    warp_days_forward(&mut setup.svm, DURATION_DAYS as i64);

    // A raise that met its target closes only after the maker claims it.
    assert_error(close_fundraiser(&mut setup), FundraiserError::TargetMet);
    assert_eq!(
        get_token_account_balance(&setup.svm, &setup.vault).unwrap(),
        AMOUNT_TO_RAISE
    );
}

#[test]
fn test_close_fundraiser_with_open_contributions_fails() {
    let mut setup = full_setup();
    initialize_fundraiser(&mut setup, AMOUNT_TO_RAISE, DURATION_DAYS);
    let contributors = fund_to_target(&mut setup);
    claim(&mut setup).unwrap();

    // Close all but one contribution account.
    let maker = setup.maker.insecure_clone();
    for contributor in &contributors[1..] {
        close_contribution(&mut setup, &maker, contributor).unwrap();
    }

    assert_error(
        close_fundraiser(&mut setup),
        FundraiserError::ContributionsOpen,
    );
    assert!(setup.svm.get_account(&setup.fundraiser_pda).is_some());
}

#[test]
fn test_reinitialize_with_open_contributions_fails() {
    let mut setup = full_setup();
    initialize_fundraiser(&mut setup, AMOUNT_TO_RAISE, DURATION_DAYS);
    fund_to_target(&mut setup);
    claim(&mut setup).unwrap();

    // The claimed fundraiser account still exists, so a new fundraiser
    // cannot be initialized at its address.
    setup.svm.expire_blockhash();
    let initialize_instruction = Instruction::new_with_bytes(
        setup.program_id,
        &fundraiser::instruction::InitializeFundraiser {
            amount: AMOUNT_TO_RAISE,
            duration: DURATION_DAYS,
        }
        .data(),
        fundraiser::accounts::InitializeFundraiserAccountConstraints {
            maker: setup.maker.pubkey(),
            mint_to_raise: setup.mint,
            fundraiser: setup.fundraiser_pda,
            vault: setup.vault,
            system_program: system_program::id(),
            token_program: token_program_id(),
            associated_token_program: ata_program_id(),
        }
        .to_account_metas(None),
    );
    let result = send_transaction_from_instructions(
        &mut setup.svm,
        vec![initialize_instruction],
        &[&setup.maker],
        &setup.maker.pubkey(),
    );
    assert!(
        result.is_err(),
        "A new fundraiser must not start while the claimed one exists"
    );
    let fundraiser_state = read_fundraiser_state(&setup.svm, &setup.fundraiser_pda);
    assert!(fundraiser_state.claimed);
}

#[test]
fn test_stale_contribution_cannot_refund_from_next_raise() {
    let mut setup = full_setup();

    // Raise one succeeds and the maker claims it.
    initialize_fundraiser(&mut setup, AMOUNT_TO_RAISE, DURATION_DAYS);
    let first_raise_contributors = fund_to_target(&mut setup);
    claim(&mut setup).unwrap();

    // The maker closes every contribution account from raise one, then the
    // fundraiser, and starts raise two at the same address.
    let maker = setup.maker.insecure_clone();
    for contributor in &first_raise_contributors {
        close_contribution(&mut setup, &maker, contributor).unwrap();
    }
    close_fundraiser(&mut setup).unwrap();
    setup.svm.expire_blockhash();
    initialize_fundraiser(&mut setup, AMOUNT_TO_RAISE, DURATION_DAYS);

    // Raise two collects less than the target and fails.
    let second_raise_contributors: Vec<FundedContributor> = (0..2)
        .map(|_| {
            let contributor = new_contributor(&mut setup);
            contribute(&mut setup, &contributor, CONTRIBUTION).unwrap();
            contributor
        })
        .collect();
    warp_days_forward(&mut setup.svm, DURATION_DAYS as i64);

    // A raise-one contributor tries to take a refund from raise two. Their
    // contribution account was closed with raise one, so there is nothing to
    // refund.
    let stale_contributor = &first_raise_contributors[0];
    let fee_payer = stale_contributor.keypair.insecure_clone();
    assert!(refund(&mut setup, &fee_payer, stale_contributor).is_err());
    assert_eq!(
        get_token_account_balance(&setup.svm, &setup.vault).unwrap(),
        2 * CONTRIBUTION
    );

    // Every raise-two contributor is refunded in full.
    for contributor in &second_raise_contributors {
        let fee_payer = contributor.keypair.insecure_clone();
        refund(&mut setup, &fee_payer, contributor).unwrap();
        assert_eq!(
            get_token_account_balance(&setup.svm, &contributor.ata).unwrap(),
            CONTRIBUTOR_STARTING_BALANCE
        );
    }
    let fundraiser_state = read_fundraiser_state(&setup.svm, &setup.fundraiser_pda);
    assert_eq!(fundraiser_state.current_amount, 0);
    assert_eq!(fundraiser_state.open_contributions, 0);
}

#[test]
fn test_close_fundraiser_after_claim_allows_a_new_raise() {
    let mut setup = full_setup();
    initialize_fundraiser(&mut setup, AMOUNT_TO_RAISE, DURATION_DAYS);
    let contributors = fund_to_target(&mut setup);
    claim(&mut setup).unwrap();

    let maker = setup.maker.insecure_clone();
    for contributor in &contributors {
        close_contribution(&mut setup, &maker, contributor).unwrap();
    }
    close_fundraiser(&mut setup).unwrap();
    assert!(setup.svm.get_account(&setup.fundraiser_pda).is_none());
    assert!(setup.svm.get_account(&setup.vault).is_none());

    setup.svm.expire_blockhash();
    initialize_fundraiser(&mut setup, AMOUNT_TO_RAISE, DURATION_DAYS);
    let fundraiser_state = read_fundraiser_state(&setup.svm, &setup.fundraiser_pda);
    assert!(!fundraiser_state.claimed);
    assert_eq!(fundraiser_state.current_amount, 0);
}

#[test]
fn test_close_fundraiser_sweeps_direct_donations_to_maker() {
    let mut setup = full_setup();
    initialize_fundraiser(&mut setup, AMOUNT_TO_RAISE, DURATION_DAYS);

    // Tokens sent straight to the vault are outside the program's
    // accounting; on close they go to the maker instead of being burned
    // with the account.
    let donation = 5 * ONE_TOKEN;
    mint_tokens_to_token_account(
        &mut setup.svm,
        &setup.mint,
        &setup.vault,
        donation,
        &setup.payer,
    )
    .unwrap();

    warp_days_forward(&mut setup.svm, DURATION_DAYS as i64);
    close_fundraiser(&mut setup).unwrap();

    let maker_ata = derive_ata(&setup.maker.pubkey(), &setup.mint);
    assert_eq!(
        get_token_account_balance(&setup.svm, &maker_ata).unwrap(),
        donation
    );
    assert!(setup.svm.get_account(&setup.fundraiser_pda).is_none());
    assert!(setup.svm.get_account(&setup.vault).is_none());
}
