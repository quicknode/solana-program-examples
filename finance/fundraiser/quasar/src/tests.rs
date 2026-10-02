//! quasar-test integration tests: create a fundraiser, contribute inside the
//! window, refund after a failed raise, pay the maker after a successful one,
//! close every contributor account and then the fundraiser, and raise again
//! at the same address — plus the deadline, target, claim, and
//! account-binding guard rails.

use {
    crate::{
        cpi::{
            CheckContributionsInstruction, CloseContributorInstruction, CloseFundraiserInstruction,
            ContributeInstruction, InitializeFundraiserInstruction, RefundInstruction,
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
/// A second, different contribution below the target.
const SECOND_PARTIAL_CONTRIBUTION: u64 = 1_300;
/// Three unequal contributions that sum to exactly the target.
const CONTRIBUTIONS_REACHING_TARGET: [u64; 3] = [4_700, 3_100, 2_200];
/// Tokens sent straight to the vault, outside `contribute`.
const DONATION: u64 = 750;

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

/// A contributor's wallet and their token account in the raised mint.
#[derive(Clone, Copy)]
struct ContributorKeys {
    wallet: Pubkey,
    token_account: Pubkey,
}

const FIRST_CONTRIBUTOR: ContributorKeys = ContributorKeys {
    wallet: CONTRIBUTOR,
    token_account: CONTRIBUTOR_TA,
};
const SECOND_CONTRIBUTOR: ContributorKeys = ContributorKeys {
    wallet: Pubkey::new_from_array([10; 32]),
    token_account: Pubkey::new_from_array([11; 32]),
};
const THIRD_CONTRIBUTOR: ContributorKeys = ContributorKeys {
    wallet: Pubkey::new_from_array([12; 32]),
    token_account: Pubkey::new_from_array([13; 32]),
};
/// Arrives after the claim.
const LATE_CONTRIBUTOR: ContributorKeys = ContributorKeys {
    wallet: Pubkey::new_from_array([14; 32]),
    token_account: Pubkey::new_from_array([15; 32]),
};
/// Contributors to a second raise at the same fundraiser address.
const NEXT_RAISE_CONTRIBUTORS: [ContributorKeys; 2] = [
    ContributorKeys {
        wallet: Pubkey::new_from_array([16; 32]),
        token_account: Pubkey::new_from_array([17; 32]),
    },
    ContributorKeys {
        wallet: Pubkey::new_from_array([18; 32]),
        token_account: Pubkey::new_from_array([19; 32]),
    },
];
/// The contributors whose `CONTRIBUTIONS_REACHING_TARGET` fund a raise.
const TARGET_CONTRIBUTORS: [ContributorKeys; 3] =
    [FIRST_CONTRIBUTOR, SECOND_CONTRIBUTOR, THIRD_CONTRIBUTOR];

fn framework_error(error: QuasarError) -> ProgramError {
    ProgramError::Custom(error as u32)
}

/// Register the maker, the maker's token account, the mint, and warp to the
/// fixed start time.
fn base_world(test: &mut Test) {
    test.add(Wallet::new().at(MAKER));
    test.add(Mint::new(MAKER).at(MINT).supply(1_000_000_000).decimals(9));
    test.add(TokenAccount::new(MINT, MAKER).at(MAKER_TA));
    test.warp_to_timestamp(START_TIME);
}

fn initialize_fundraiser(test: &mut Test, amount_to_raise: u64, duration: u16) -> Outcome {
    test.send(InitializeFundraiserInstruction {
        maker: MAKER,
        mint_to_raise: MINT,
        vault: VAULT,
        amount_to_raise,
        duration,
    })
}

/// Give a contributor a wallet and a funded token account.
fn add_contributor(test: &mut Test, contributor: ContributorKeys) {
    test.add(Wallet::new().at(contributor.wallet));
    test.add(
        TokenAccount::new(MINT, contributor.wallet)
            .at(contributor.token_account)
            .amount(CONTRIBUTOR_STARTING_BALANCE),
    );
}

/// A world with an initialized fundraiser and a funded contributor.
fn initialized_world(test: &mut Test) -> Pubkey {
    base_world(test);
    initialize_fundraiser(test, TARGET_AMOUNT, DURATION_DAYS).succeeds();
    add_contributor(test, FIRST_CONTRIBUTOR);
    test.derive_pda(Fundraiser::seeds(&MAKER))
}

fn contributor_account(test: &Test, fundraiser: Pubkey, contributor: ContributorKeys) -> Pubkey {
    test.derive_pda(Contributor::seeds(&fundraiser, &contributor.wallet))
}

fn contribute_from(test: &mut Test, contributor: ContributorKeys, amount: u64) -> Outcome {
    test.send(ContributeInstruction {
        contributor: contributor.wallet,
        maker: MAKER,
        contributor_ta: contributor.token_account,
        vault: VAULT,
        mint_to_raise: MINT,
        amount,
    })
}

fn contribute(test: &mut Test, amount: u64) -> Outcome {
    contribute_from(test, FIRST_CONTRIBUTOR, amount)
}

/// Three contributors whose contributions reach the target exactly.
fn fund_to_target(test: &mut Test) {
    for (contributor, amount) in TARGET_CONTRIBUTORS
        .iter()
        .zip(CONTRIBUTIONS_REACHING_TARGET)
    {
        if contributor.wallet != CONTRIBUTOR {
            add_contributor(test, *contributor);
        }
        contribute_from(test, *contributor, amount).succeeds();
    }
}

fn refund_instruction(contributor: ContributorKeys) -> Instruction {
    RefundInstruction {
        contributor: contributor.wallet,
        maker: MAKER,
        contributor_ta: contributor.token_account,
        vault: VAULT,
        mint_to_raise: MINT,
    }
    .into()
}

fn refund_for(test: &mut Test, contributor: ContributorKeys) -> Outcome {
    test.send(refund_instruction(contributor))
}

fn refund(test: &mut Test) -> Outcome {
    refund_for(test, FIRST_CONTRIBUTOR)
}

fn check_contributions(test: &mut Test) -> Outcome {
    test.send(CheckContributionsInstruction {
        maker: MAKER,
        vault: VAULT,
        maker_ta: MAKER_TA,
        mint_to_raise: MINT,
    })
}

fn close_contributor_instruction(contributor: ContributorKeys, fundraiser: Pubkey) -> Instruction {
    CloseContributorInstruction {
        contributor: contributor.wallet,
        fundraiser,
    }
    .into()
}

fn close_contributor_for(
    test: &mut Test,
    contributor: ContributorKeys,
    fundraiser: Pubkey,
) -> Outcome {
    test.send(close_contributor_instruction(contributor, fundraiser))
}

fn close_contributor(test: &mut Test, fundraiser: Pubkey) -> Outcome {
    close_contributor_for(test, FIRST_CONTRIBUTOR, fundraiser)
}

fn close_fundraiser(test: &mut Test) -> Outcome {
    test.send(CloseFundraiserInstruction {
        maker: MAKER,
        vault: VAULT,
        maker_ta: MAKER_TA,
        mint_to_raise: MINT,
    })
}

/// Send tokens straight to the vault, bypassing `contribute`, by rewriting
/// the vault's balance.
fn donate_to_vault(test: &mut Test, fundraiser: Pubkey, amount: u64) {
    let balance = test.tokens(VAULT);
    test.add(
        TokenAccount::new(MINT, fundraiser)
            .at(VAULT)
            .amount(balance + amount),
    );
}

fn open_contributor_accounts(test: &Test, fundraiser: Pubkey) -> u32 {
    u32::from(
        test.read::<Fundraiser>(fundraiser)
            .open_contributor_accounts,
    )
}

fn is_claimed(test: &Test, fundraiser: Pubkey) -> bool {
    bool::from(test.read::<Fundraiser>(fundraiser).claimed)
}

#[quasar_test]
fn initialize_records_state_and_clock_time(test: &mut Test) {
    base_world(test);
    initialize_fundraiser(test, TARGET_AMOUNT, DURATION_DAYS)
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
    assert!(!bool::from(state.claimed));
    assert_eq!(u32::from(state.open_contributor_accounts), 0);
    assert_eq!(state.bump, expected_bump);
}

#[quasar_test]
fn initialize_rejects_zero_amount(test: &mut Test) {
    base_world(test);
    initialize_fundraiser(test, 0, DURATION_DAYS).fails_with(FundraiserError::InvalidAmount);
}

#[quasar_test]
fn initialize_rejects_zero_duration(test: &mut Test) {
    base_world(test);
    initialize_fundraiser(test, TARGET_AMOUNT, 0).fails_with(FundraiserError::InvalidDuration);
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
    assert_eq!(
        u64::from(fundraiser_state.current_amount),
        PARTIAL_CONTRIBUTION
    );
    assert_eq!(u32::from(fundraiser_state.open_contributor_accounts), 1);

    let (contributor_account, expected_bump) =
        test.derive_pda_with_bump(Contributor::seeds(&fundraiser, &CONTRIBUTOR));
    let contributor_state = test.read::<Contributor>(contributor_account);
    assert_eq!(u64::from(contributor_state.amount), PARTIAL_CONTRIBUTION);
    assert_eq!(contributor_state.bump, expected_bump);
}

#[quasar_test]
fn contributions_accumulate_in_one_contributor_account(test: &mut Test) {
    let fundraiser = initialized_world(test);
    contribute(test, PARTIAL_CONTRIBUTION).succeeds();

    // The second contribution reuses the contributor account created by the
    // first, so the fundraiser still counts one open contributor account.
    let expected_total = PARTIAL_CONTRIBUTION + SECOND_PARTIAL_CONTRIBUTION;
    contribute(test, SECOND_PARTIAL_CONTRIBUTION)
        .succeeds()
        .has_tokens(VAULT, expected_total);

    let contributor_account = contributor_account(test, fundraiser, FIRST_CONTRIBUTOR);
    assert_eq!(
        u64::from(test.read::<Contributor>(contributor_account).amount),
        expected_total
    );
    assert_eq!(
        u64::from(test.read::<Fundraiser>(fundraiser).current_amount),
        expected_total
    );
    assert_eq!(open_contributor_accounts(test, fundraiser), 1);
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
fn contribute_after_claim_fails(test: &mut Test) {
    let fundraiser = initialized_world(test);
    fund_to_target(test);
    check_contributions(test).succeeds();

    // The deadline is still days away, but the vault has been paid out.
    test.warp_to_timestamp(START_TIME + SECONDS_PER_DAY);
    add_contributor(test, LATE_CONTRIBUTOR);
    contribute_from(test, LATE_CONTRIBUTOR, PARTIAL_CONTRIBUTION)
        .fails_with(FundraiserError::FundraiserClaimed);

    assert_eq!(
        test.tokens(LATE_CONTRIBUTOR.token_account),
        CONTRIBUTOR_STARTING_BALANCE
    );
    assert_eq!(
        open_contributor_accounts(test, fundraiser),
        TARGET_CONTRIBUTORS.len() as u32
    );
}

#[quasar_test]
fn refund_returns_tokens_after_failed_fundraiser(test: &mut Test) {
    let fundraiser = initialized_world(test);
    contribute(test, PARTIAL_CONTRIBUTION).succeeds();

    test.warp_to_timestamp(DEADLINE);

    let contributor_account = contributor_account(test, fundraiser, FIRST_CONTRIBUTOR);
    refund(test)
        .succeeds()
        .has_tokens(VAULT, 0)
        .has_tokens(CONTRIBUTOR_TA, CONTRIBUTOR_STARTING_BALANCE)
        // The contributor account was closed and its rent returned.
        .is_closed(contributor_account);

    let fundraiser_state = test.read::<Fundraiser>(fundraiser);
    assert_eq!(u64::from(fundraiser_state.current_amount), 0);
    assert_eq!(u32::from(fundraiser_state.open_contributor_accounts), 0);
}

#[quasar_test]
fn anyone_can_refund_a_contributor(test: &mut Test) {
    let fundraiser = initialized_world(test);
    contribute(test, SECOND_PARTIAL_CONTRIBUTION).succeeds();
    test.warp_to_timestamp(DEADLINE);

    // No account in the refund is a signer: whoever sends it, the tokens
    // and the rent go to the contributor, who signs nothing.
    let instruction = refund_instruction(FIRST_CONTRIBUTOR);
    assert!(
        instruction.accounts.iter().all(|meta| !meta.is_signer),
        "refund must not require any signature"
    );

    let contributor_account = contributor_account(test, fundraiser, FIRST_CONTRIBUTOR);
    let rent = test.lamports(contributor_account);
    let contributor_lamports_before = test.lamports(CONTRIBUTOR);

    test.send(instruction)
        .succeeds()
        .has_tokens(CONTRIBUTOR_TA, CONTRIBUTOR_STARTING_BALANCE)
        .is_closed(contributor_account);
    assert_eq!(
        test.lamports(CONTRIBUTOR),
        contributor_lamports_before + rent
    );
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
    fund_to_target(test);

    test.warp_to_timestamp(DEADLINE);
    refund(test).fails_with(FundraiserError::TargetMet);
    assert_eq!(test.tokens(VAULT), TARGET_AMOUNT);
}

#[quasar_test]
fn refund_rejects_another_contributors_account(test: &mut Test) {
    initialized_world(test);
    contribute(test, PARTIAL_CONTRIBUTION).succeeds();

    test.warp_to_timestamp(DEADLINE);

    test.add(Wallet::new().at(ATTACKER));
    test.add(TokenAccount::new(MINT, ATTACKER).at(ATTACKER_TA));

    // Refunds need no signature, so the attacker names the victim as the
    // contributor but routes the tokens to their own token account. The
    // destination must be owned by the contributor.
    let mut instruction = refund_instruction(FIRST_CONTRIBUTOR);
    // Account indices follow the accounts-struct field order:
    // 0 contributor, 3 contributor_account, 4 contributor_ta.
    instruction.accounts[4].pubkey = ATTACKER_TA;
    test.send(instruction)
        .fails(ProgramError::InvalidAccountData);

    // The attacker names themselves as the contributor, with their own token
    // account, against the victim's contributor record. The record's PDA is
    // derived from ["contributor", fundraiser, attacker], which does not
    // match.
    let mut instruction = refund_instruction(FIRST_CONTRIBUTOR);
    instruction.accounts[0].pubkey = ATTACKER;
    instruction.accounts[4].pubkey = ATTACKER_TA;
    test.send(instruction)
        .fails(framework_error(QuasarError::InvalidPda));

    // The vault still holds the victim's contribution.
    assert_eq!(test.tokens(VAULT), PARTIAL_CONTRIBUTION);
    assert_eq!(test.tokens(ATTACKER_TA), 0);
}

#[quasar_test]
fn check_contributions_pays_maker_and_marks_claimed(test: &mut Test) {
    let fundraiser = initialized_world(test);
    fund_to_target(test);

    check_contributions(test)
        .succeeds()
        .has_tokens(MAKER_TA, TARGET_AMOUNT)
        .has_tokens(VAULT, 0);

    // The fundraiser and the vault stay open, the fundraiser marked claimed,
    // until every contributor account written for it is closed.
    assert!(
        test.account(VAULT).is_some(),
        "the vault survives the claim"
    );
    assert!(is_claimed(test, fundraiser));
    assert_eq!(
        open_contributor_accounts(test, fundraiser),
        TARGET_CONTRIBUTORS.len() as u32
    );
}

#[quasar_test]
fn check_contributions_rejected_below_target(test: &mut Test) {
    initialized_world(test);
    contribute(test, PARTIAL_CONTRIBUTION).succeeds();

    check_contributions(test).fails_with(FundraiserError::TargetNotMet);
}

#[quasar_test]
fn check_contributions_ignores_direct_vault_donations(test: &mut Test) {
    let fundraiser = initialized_world(test);

    // The full target sent straight to the vault leaves the state-tracked
    // current_amount at 0, so the claim must fail.
    donate_to_vault(test, fundraiser, TARGET_AMOUNT);

    check_contributions(test).fails_with(FundraiserError::TargetNotMet);
    assert!(!is_claimed(test, fundraiser));
}

#[quasar_test]
fn second_claim_fails(test: &mut Test) {
    let fundraiser = initialized_world(test);
    fund_to_target(test);
    check_contributions(test).succeeds();

    // A donation to the vault after the claim must not make a second claim
    // possible.
    donate_to_vault(test, fundraiser, DONATION);

    check_contributions(test).fails_with(FundraiserError::FundraiserClaimed);
    assert_eq!(test.tokens(MAKER_TA), TARGET_AMOUNT);
    assert_eq!(test.tokens(VAULT), DONATION);
}

#[quasar_test]
fn close_contributor_returns_rent_after_successful_raise(test: &mut Test) {
    let fundraiser = initialized_world(test);
    fund_to_target(test);
    check_contributions(test).succeeds();

    // The claim leaves the contributor account open with its rent inside.
    let contributor_account = contributor_account(test, fundraiser, FIRST_CONTRIBUTOR);
    let rent = test.lamports(contributor_account);
    assert!(rent > 0, "the contributor account survives the claim");
    let lamports_before = test.lamports(CONTRIBUTOR);

    close_contributor(test, fundraiser)
        .succeeds()
        .is_closed(contributor_account);
    assert_eq!(
        test.lamports(CONTRIBUTOR),
        lamports_before + rent,
        "the contributor account's rent returns to the contributor"
    );
    assert_eq!(
        open_contributor_accounts(test, fundraiser),
        TARGET_CONTRIBUTORS.len() as u32 - 1
    );
}

#[quasar_test]
fn anyone_can_close_contributor_accounts_after_claim(test: &mut Test) {
    let fundraiser = initialized_world(test);
    fund_to_target(test);
    check_contributions(test).succeeds();

    // Whoever sends it, each rent deposit goes to its contributor, who signs
    // nothing.
    for contributor in TARGET_CONTRIBUTORS {
        let instruction = close_contributor_instruction(contributor, fundraiser);
        assert!(
            instruction.accounts.iter().all(|meta| !meta.is_signer),
            "close_contributor must not require any signature"
        );

        let contributor_account = contributor_account(test, fundraiser, contributor);
        let rent = test.lamports(contributor_account);
        let lamports_before = test.lamports(contributor.wallet);
        test.send(instruction)
            .succeeds()
            .is_closed(contributor_account);
        assert_eq!(test.lamports(contributor.wallet), lamports_before + rent);
    }

    assert_eq!(open_contributor_accounts(test, fundraiser), 0);
}

#[quasar_test]
fn close_contributor_before_claim_fails(test: &mut Test) {
    let fundraiser = initialized_world(test);
    contribute(test, PARTIAL_CONTRIBUTION).succeeds();

    // The fundraiser is unclaimed, so the contribution can still be
    // refunded: closing the record now would erase what the vault owes.
    close_contributor(test, fundraiser).fails_with(FundraiserError::FundraiserNotClaimed);

    let contributor_account = contributor_account(test, fundraiser, FIRST_CONTRIBUTOR);
    assert_eq!(
        u64::from(test.read::<Contributor>(contributor_account).amount),
        PARTIAL_CONTRIBUTION
    );
    assert_eq!(open_contributor_accounts(test, fundraiser), 1);
}

#[quasar_test]
fn close_fundraiser_after_failed_raise_allows_a_new_raise(test: &mut Test) {
    let fundraiser = initialized_world(test);
    contribute(test, PARTIAL_CONTRIBUTION).succeeds();

    // The raise fails; the contributor takes their refund.
    test.warp_to_timestamp(DEADLINE);
    refund(test).succeeds();

    close_fundraiser(test)
        .succeeds()
        .is_closed(VAULT)
        .is_closed(fundraiser);

    // The same maker can now open a fresh fundraiser at the same address.
    initialize_fundraiser(test, TARGET_AMOUNT, DURATION_DAYS).succeeds();
    let state = test.read::<Fundraiser>(fundraiser);
    assert_eq!(u64::from(state.current_amount), 0);
    assert_eq!(u64::from(state.amount_to_raise), TARGET_AMOUNT);
    assert_eq!(i64::from(state.time_started), DEADLINE);
    assert_eq!(test.tokens(VAULT), 0);
}

#[quasar_test]
fn close_fundraiser_after_claim_allows_a_new_raise(test: &mut Test) {
    let fundraiser = initialized_world(test);
    fund_to_target(test);
    check_contributions(test).succeeds();

    for contributor in TARGET_CONTRIBUTORS {
        close_contributor_for(test, contributor, fundraiser).succeeds();
    }
    // A claimed fundraiser closes without waiting for its deadline.
    close_fundraiser(test)
        .succeeds()
        .is_closed(VAULT)
        .is_closed(fundraiser);

    initialize_fundraiser(test, TARGET_AMOUNT, DURATION_DAYS).succeeds();
    assert!(!is_claimed(test, fundraiser));
    assert_eq!(
        u64::from(test.read::<Fundraiser>(fundraiser).current_amount),
        0
    );
}

#[quasar_test]
fn close_fundraiser_before_deadline_fails(test: &mut Test) {
    let fundraiser = initialized_world(test);

    // One second short of the deadline.
    test.warp_to_timestamp(DEADLINE - 1);

    close_fundraiser(test).fails_with(FundraiserError::FundraiserNotEnded);
    assert!(test.account(fundraiser).is_some());
}

#[quasar_test]
fn close_fundraiser_with_unrefunded_contributions_fails(test: &mut Test) {
    initialized_world(test);
    contribute(test, PARTIAL_CONTRIBUTION).succeeds();

    // Past the deadline but the contribution has not been refunded, so
    // closing would strand it in the vault.
    test.warp_to_timestamp(DEADLINE);

    close_fundraiser(test).fails_with(FundraiserError::RefundsOutstanding);
    assert_eq!(test.tokens(VAULT), PARTIAL_CONTRIBUTION);
}

#[quasar_test]
fn close_fundraiser_when_target_met_but_unclaimed_fails(test: &mut Test) {
    initialized_world(test);
    fund_to_target(test);

    test.warp_to_timestamp(DEADLINE);

    // A raise that met its target closes only after the maker claims it.
    close_fundraiser(test).fails_with(FundraiserError::TargetMet);
    assert_eq!(test.tokens(VAULT), TARGET_AMOUNT);
}

#[quasar_test]
fn close_fundraiser_with_open_contributor_accounts_fails(test: &mut Test) {
    let fundraiser = initialized_world(test);
    fund_to_target(test);
    check_contributions(test).succeeds();

    // Close all but the first contributor account.
    for contributor in &TARGET_CONTRIBUTORS[1..] {
        close_contributor_for(test, *contributor, fundraiser).succeeds();
    }

    close_fundraiser(test).fails_with(FundraiserError::ContributorAccountsOpen);
    assert!(test.account(fundraiser).is_some());
    assert_eq!(open_contributor_accounts(test, fundraiser), 1);
}

#[quasar_test]
fn reinitialize_with_open_contributor_accounts_fails(test: &mut Test) {
    let fundraiser = initialized_world(test);
    fund_to_target(test);
    check_contributions(test).succeeds();

    // The claimed fundraiser account still exists, so a new fundraiser
    // cannot be initialized at its address.
    initialize_fundraiser(test, TARGET_AMOUNT, DURATION_DAYS)
        .fails(ProgramError::AccountAlreadyInitialized);
    assert!(is_claimed(test, fundraiser));
    assert_eq!(
        open_contributor_accounts(test, fundraiser),
        TARGET_CONTRIBUTORS.len() as u32
    );
}

#[quasar_test]
fn stale_contributor_account_cannot_refund_from_next_raise(test: &mut Test) {
    let fundraiser = initialized_world(test);

    // Raise one succeeds and the maker claims it.
    fund_to_target(test);
    check_contributions(test).succeeds();

    // The maker closes every contributor account from raise one, then the
    // fundraiser, and starts raise two at the same address.
    for contributor in TARGET_CONTRIBUTORS {
        close_contributor_for(test, contributor, fundraiser).succeeds();
    }
    close_fundraiser(test).succeeds();
    initialize_fundraiser(test, TARGET_AMOUNT, DURATION_DAYS).succeeds();

    // Raise two collects less than the target and fails.
    let next_raise_amounts = [PARTIAL_CONTRIBUTION, SECOND_PARTIAL_CONTRIBUTION];
    for (contributor, amount) in NEXT_RAISE_CONTRIBUTORS.iter().zip(next_raise_amounts) {
        add_contributor(test, *contributor);
        contribute_from(test, *contributor, amount).succeeds();
    }
    let raise_two_total = PARTIAL_CONTRIBUTION + SECOND_PARTIAL_CONTRIBUTION;
    assert_eq!(open_contributor_accounts(test, fundraiser), 2);
    test.warp_to_timestamp(DEADLINE);

    // A raise-one contributor tries to take a refund from raise two. Their
    // contributor account was closed with raise one, so the address is a
    // system-owned empty account, not a contributor record: there is nothing
    // to refund.
    let stale_contributor = FIRST_CONTRIBUTOR;
    let stale_balance = test.tokens(stale_contributor.token_account);
    // The runtime reports the wrong-owner check as `IllegalOwner`, outside
    // quasar-test's stable error set.
    refund_for(test, stale_contributor).fails(ProgramError::Runtime("IllegalOwner".into()));
    assert_eq!(test.tokens(VAULT), raise_two_total);
    assert_eq!(test.tokens(stale_contributor.token_account), stale_balance);

    // Every raise-two contributor is refunded in full.
    for contributor in NEXT_RAISE_CONTRIBUTORS {
        refund_for(test, contributor)
            .succeeds()
            .has_tokens(contributor.token_account, CONTRIBUTOR_STARTING_BALANCE);
    }
    let state = test.read::<Fundraiser>(fundraiser);
    assert_eq!(u64::from(state.current_amount), 0);
    assert_eq!(u32::from(state.open_contributor_accounts), 0);
    assert_eq!(test.tokens(VAULT), 0);
}

#[quasar_test]
fn close_fundraiser_sweeps_direct_donations_to_maker(test: &mut Test) {
    let fundraiser = initialized_world(test);

    // Tokens sent straight to the vault are outside the program's
    // accounting; on close they go to the maker instead of being burned
    // with the account.
    donate_to_vault(test, fundraiser, DONATION);

    test.warp_to_timestamp(DEADLINE);
    close_fundraiser(test)
        .succeeds()
        .has_tokens(MAKER_TA, DONATION)
        .is_closed(VAULT)
        .is_closed(fundraiser);
}
