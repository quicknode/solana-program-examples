//! quasar-test integration tests: create a fundraiser, contribute inside the
//! window, refund after a failed raise, and pay the maker after a successful
//! one — plus the deadline, target, and account-binding guard rails.

use {
    crate::{
        cpi::{
            CheckContributionsInstruction, ContributeInstruction, InitializeInstruction,
            RefundInstruction,
        },
        error::FundraiserError,
        state::{Contributor, Fundraiser, SECONDS_PER_DAY},
    },
    quasar_lang::error::QuasarError,
    quasar_test::prelude::*,
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

// Deterministic addresses.
const MAKER: Pubkey = Pubkey::new_from_array([1; 32]);
const MINT: Pubkey = Pubkey::new_from_array([2; 32]);
const VAULT: Pubkey = Pubkey::new_from_array([3; 32]);
const CONTRIBUTOR: Pubkey = Pubkey::new_from_array([4; 32]);
const CONTRIBUTOR_TA: Pubkey = Pubkey::new_from_array([5; 32]);
const MAKER_TA: Pubkey = Pubkey::new_from_array([6; 32]);
const ATTACKER: Pubkey = Pubkey::new_from_array([7; 32]);
const ATTACKER_TA: Pubkey = Pubkey::new_from_array([8; 32]);
const DECOY_VAULT: Pubkey = Pubkey::new_from_array([9; 32]);

fn framework_error(error: QuasarError) -> ProgramError {
    ProgramError::Custom(error as u32)
}

/// Register the maker, the mint, and warp to the fixed start time.
fn base_world(test: &mut Test) {
    test.add(Wallet::new().at(MAKER));
    test.add(
        Mint::new(MAKER)
            .at(MINT)
            .supply(1_000_000_000)
            .decimals(9),
    );
    test.warp_to_timestamp(START_TIME);
}

fn initialize(test: &mut Test, amount_to_raise: u64, duration: u16) -> Outcome {
    test.send(InitializeInstruction {
        maker: MAKER,
        mint_to_raise: MINT,
        vault: VAULT,
        amount_to_raise,
        duration,
    })
}

/// A world with an initialized fundraiser and a funded contributor.
fn initialized_world(test: &mut Test) -> Pubkey {
    base_world(test);
    initialize(test, TARGET_AMOUNT, DURATION_DAYS).succeeds();
    test.add(Wallet::new().at(CONTRIBUTOR));
    test.add(
        TokenAccount::new(MINT, CONTRIBUTOR)
            .at(CONTRIBUTOR_TA)
            .amount(CONTRIBUTOR_STARTING_BALANCE),
    );
    test.derive_pda(Fundraiser::seeds(&MAKER))
}

fn contribute(test: &mut Test, amount: u64) -> Outcome {
    test.send(ContributeInstruction {
        contributor: CONTRIBUTOR,
        maker: MAKER,
        contributor_ta: CONTRIBUTOR_TA,
        vault: VAULT,
        mint_to_raise: MINT,
        amount,
    })
}

fn refund(test: &mut Test) -> Outcome {
    test.send(RefundInstruction {
        contributor: CONTRIBUTOR,
        maker: MAKER,
        contributor_ta: CONTRIBUTOR_TA,
        vault: VAULT,
        mint_to_raise: MINT,
    })
}

fn check_contributions(test: &mut Test) -> Outcome {
    test.send(CheckContributionsInstruction {
        maker: MAKER,
        vault: VAULT,
        maker_ta: MAKER_TA,
        mint_to_raise: MINT,
    })
}

#[quasar_test]
fn initialize_records_state_and_clock_time(test: &mut Test) {
    base_world(test);
    initialize(test, TARGET_AMOUNT, DURATION_DAYS)
        .succeeds()
        .has_tokens(VAULT, 0);

    let (fundraiser, expected_bump) = test.derive_pda_with_bump(Fundraiser::seeds(&MAKER));
    let state = test.read::<Fundraiser>(fundraiser);
    assert_eq!(state.maker, MAKER);
    assert_eq!(state.mint_to_raise, MINT);
    assert_eq!(state.vault, VAULT);
    assert_eq!(u64::from(state.amount_to_raise), TARGET_AMOUNT);
    assert_eq!(u64::from(state.current_amount), 0);
    assert_eq!(i64::from(state.time_started), START_TIME);
    assert_eq!(u16::from(state.duration), DURATION_DAYS);
    assert_eq!(state.bump, expected_bump);
}

#[quasar_test]
fn initialize_rejects_zero_amount(test: &mut Test) {
    base_world(test);
    initialize(test, 0, DURATION_DAYS).fails_with(FundraiserError::InvalidAmount);
}

#[quasar_test]
fn initialize_rejects_zero_duration(test: &mut Test) {
    base_world(test);
    initialize(test, TARGET_AMOUNT, 0).fails_with(FundraiserError::InvalidDuration);
}

#[quasar_test]
fn contribute_creates_contributor_account_and_moves_tokens(test: &mut Test) {
    let fundraiser = initialized_world(test);

    contribute(test, PARTIAL_CONTRIBUTION)
        .succeeds()
        .has_tokens(VAULT, PARTIAL_CONTRIBUTION)
        .has_tokens(
            CONTRIBUTOR_TA,
            CONTRIBUTOR_STARTING_BALANCE - PARTIAL_CONTRIBUTION,
        );

    let fundraiser_state = test.read::<Fundraiser>(fundraiser);
    assert_eq!(u64::from(fundraiser_state.current_amount), PARTIAL_CONTRIBUTION);

    let (contributor_account, expected_bump) =
        test.derive_pda_with_bump(Contributor::seeds(&fundraiser, &CONTRIBUTOR));
    let contributor_state = test.read::<Contributor>(contributor_account);
    assert_eq!(u64::from(contributor_state.amount), PARTIAL_CONTRIBUTION);
    assert_eq!(contributor_state.bump, expected_bump);
}

#[quasar_test]
fn contribute_accumulates_across_calls(test: &mut Test) {
    let fundraiser = initialized_world(test);
    contribute(test, PARTIAL_CONTRIBUTION).succeeds();

    // Second contribution reuses the contributor account created by the first.
    let expected_total = PARTIAL_CONTRIBUTION * 2;
    contribute(test, PARTIAL_CONTRIBUTION)
        .succeeds()
        .has_tokens(VAULT, expected_total);

    let contributor_account = test.derive_pda(Contributor::seeds(&fundraiser, &CONTRIBUTOR));
    assert_eq!(
        u64::from(test.read::<Contributor>(contributor_account).amount),
        expected_total
    );
    assert_eq!(
        u64::from(test.read::<Fundraiser>(fundraiser).current_amount),
        expected_total
    );
}

#[quasar_test]
fn contribute_rejected_after_deadline(test: &mut Test) {
    initialized_world(test);
    test.warp_to_timestamp(DEADLINE);
    contribute(test, PARTIAL_CONTRIBUTION).fails_with(FundraiserError::FundraiserEnded);
}

#[quasar_test]
fn contribute_allowed_just_before_deadline(test: &mut Test) {
    initialized_world(test);
    test.warp_to_timestamp(DEADLINE - 1);
    contribute(test, PARTIAL_CONTRIBUTION)
        .succeeds()
        .has_tokens(VAULT, PARTIAL_CONTRIBUTION);
}

#[quasar_test]
fn contribute_rejects_vault_not_bound_to_fundraiser(test: &mut Test) {
    let fundraiser = initialized_world(test);

    // The attacker tries to credit the fundraiser while depositing into a
    // decoy token account instead of the fundraiser's stored vault.
    test.add(TokenAccount::new(MINT, fundraiser).at(DECOY_VAULT));

    let mut instruction: Instruction = ContributeInstruction {
        contributor: CONTRIBUTOR,
        maker: MAKER,
        contributor_ta: CONTRIBUTOR_TA,
        vault: DECOY_VAULT,
        mint_to_raise: MINT,
        amount: PARTIAL_CONTRIBUTION,
    }
    .into();
    // Account index 5 is the vault (accounts-struct field order); the builder
    // already put the decoy there, this documents the tampered position.
    instruction.accounts[5].pubkey = DECOY_VAULT;

    test.send(instruction)
        .fails(framework_error(QuasarError::HasOneMismatch));
}

#[quasar_test]
fn refund_returns_tokens_after_failed_fundraiser(test: &mut Test) {
    let fundraiser = initialized_world(test);
    contribute(test, PARTIAL_CONTRIBUTION).succeeds();

    test.warp_to_timestamp(DEADLINE);

    let contributor_account = test.derive_pda(Contributor::seeds(&fundraiser, &CONTRIBUTOR));
    refund(test)
        .succeeds()
        .has_tokens(VAULT, 0)
        .has_tokens(CONTRIBUTOR_TA, CONTRIBUTOR_STARTING_BALANCE)
        // The contributor account was closed and its rent returned.
        .is_closed(contributor_account);

    assert_eq!(u64::from(test.read::<Fundraiser>(fundraiser).current_amount), 0);
}

#[quasar_test]
fn refund_rejected_before_deadline(test: &mut Test) {
    initialized_world(test);
    contribute(test, PARTIAL_CONTRIBUTION).succeeds();

    test.warp_to_timestamp(DEADLINE - 1);
    refund(test).fails_with(FundraiserError::FundraiserNotEnded);
}

#[quasar_test]
fn refund_rejected_when_target_met(test: &mut Test) {
    initialized_world(test);
    contribute(test, TARGET_AMOUNT).succeeds();

    test.warp_to_timestamp(DEADLINE);
    refund(test).fails_with(FundraiserError::TargetMet);
}

#[quasar_test]
fn refund_rejects_another_contributors_account(test: &mut Test) {
    initialized_world(test);
    contribute(test, PARTIAL_CONTRIBUTION).succeeds();

    test.warp_to_timestamp(DEADLINE);

    // The attacker signs as themselves but passes the victim's contributor
    // record and their own token account, trying to drain the vault.
    test.add(Wallet::new().at(ATTACKER));
    test.add(TokenAccount::new(MINT, ATTACKER).at(ATTACKER_TA));

    let mut instruction: Instruction = RefundInstruction {
        contributor: CONTRIBUTOR,
        maker: MAKER,
        contributor_ta: ATTACKER_TA,
        vault: VAULT,
        mint_to_raise: MINT,
    }
    .into();
    // Account indices follow the accounts-struct field order:
    // 0 contributor (signer), 3 contributor_account, 4 contributor_ta. The
    // builder derived contributor_account for the VICTIM; swap only the
    // signer to the attacker.
    instruction.accounts[0].pubkey = ATTACKER;

    // The contributor_account PDA check derives ["contributor", fundraiser,
    // attacker], which does not match the victim's record.
    test.send(instruction)
        .fails(framework_error(QuasarError::InvalidPda));
    // The vault still holds the victim's contribution.
    assert_eq!(test.tokens(VAULT), PARTIAL_CONTRIBUTION);
}

#[quasar_test]
fn check_contributions_pays_maker_when_target_met(test: &mut Test) {
    let fundraiser = initialized_world(test);
    contribute(test, TARGET_AMOUNT).succeeds();

    test.add(TokenAccount::new(MINT, MAKER).at(MAKER_TA));

    check_contributions(test)
        .succeeds()
        .has_tokens(MAKER_TA, TARGET_AMOUNT)
        // The vault and fundraiser accounts were closed.
        .is_closed(VAULT)
        .is_closed(fundraiser);
}

#[quasar_test]
fn check_contributions_rejected_below_target(test: &mut Test) {
    initialized_world(test);
    contribute(test, PARTIAL_CONTRIBUTION).succeeds();

    test.add(TokenAccount::new(MINT, MAKER).at(MAKER_TA));
    check_contributions(test).fails_with(FundraiserError::TargetNotMet);
}
