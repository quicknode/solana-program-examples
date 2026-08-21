use {
    anchor_lang::{
    anchor_v2_testing::{Keypair, LiteSVM, Signer},
        solana_program::instruction::Instruction, system_program, Address, InstructionData,
        ToAccountMetas,
    },
    borsh::BorshDeserialize,
    fundraiser::SECONDS_TO_DAYS,
    // LiteSVM's get_sysvar wants the host-side Clock, not pinocchio's.
    solana_clock::Clock,
    solana_kite::{
        create_associated_token_account, create_token_mint, create_wallet,
        get_token_account_balance, mint_tokens_to_token_account,
        send_transaction_from_instructions,
    },
};

const MINT_DECIMALS: u8 = 6;
/// One major unit of the test mint in minor units (10^MINT_DECIMALS).
const ONE_TOKEN: u64 = 1_000_000;
/// Comfortably above the program's 3-major-unit minimum target.
const AMOUNT_TO_RAISE: u64 = 30 * ONE_TOKEN;
/// The per-contributor cap is 10% of the target.
const MAX_CONTRIBUTION: u64 = AMOUNT_TO_RAISE / 10;
const DURATION_DAYS: u16 = 7;
const CONTRIBUTOR_STARTING_BALANCE: u64 = 10 * ONE_TOKEN;

fn token_program_id() -> Address {
    "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA"
        .parse()
        .unwrap()
}

fn ata_program_id() -> Address {
    "ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL"
        .parse()
        .unwrap()
}

fn derive_ata(wallet: &Address, mint: &Address) -> Address {
    let (ata, _bump) = Address::find_program_address(
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
    _bump: u8,
}

/// Mirror of the onchain Contributor struct.
#[derive(BorshDeserialize)]
struct ContributorState {
    amount: u64,
    _bump: u8,
}

const ANCHOR_DISCRIMINATOR_LENGTH: usize = 8;

fn read_fundraiser_state(svm: &LiteSVM, fundraiser_pda: &Address) -> FundraiserState {
    let account = svm.get_account(fundraiser_pda).unwrap();
    FundraiserState::try_from_slice(&account.data[ANCHOR_DISCRIMINATOR_LENGTH..]).unwrap()
}

fn read_contributor_state(svm: &LiteSVM, contributor_pda: &Address) -> ContributorState {
    let account = svm.get_account(contributor_pda).unwrap();
    ContributorState::try_from_slice(&account.data[ANCHOR_DISCRIMINATOR_LENGTH..]).unwrap()
}

/// Moves the LiteSVM clock forward by the given number of days.
fn warp_days_forward(svm: &mut LiteSVM, days: i64) {
    let mut clock: Clock = svm.get_sysvar();
    clock.unix_timestamp += days * SECONDS_TO_DAYS;
    svm.set_sysvar(&clock);
}

struct FundraiserSetup {
    svm: LiteSVM,
    program_id: Address,
    payer: Keypair,
    maker: Keypair,
    mint: Address,
    fundraiser_pda: Address,
    vault: Address,
}

fn full_setup() -> FundraiserSetup {
    let program_id = fundraiser::id();
    let mut svm = anchor_v2_testing::svm();

    let program_bytes = include_bytes!("../../../target/deploy/fundraiser.so");
    svm.add_program(program_id, program_bytes).unwrap();

    let payer = create_wallet(&mut svm, 100_000_000_000).unwrap();
    let maker = create_wallet(&mut svm, 10_000_000_000).unwrap();

    // The payer is the mint authority.
    let mint = create_token_mint(&mut svm, &payer, MINT_DECIMALS, None).unwrap();

    let (fundraiser_pda, _bump) =
        Address::find_program_address(&[b"fundraiser", maker.pubkey().as_ref()], &program_id);

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
            system_program: system_program::ID,
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
/// (contributor keypair, contributor ATA, contributor account PDA).
fn create_funded_contributor(setup: &mut FundraiserSetup) -> (Keypair, Address, Address) {
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

    let (contributor_account_pda, _bump) = Address::find_program_address(
        &[
            b"contributor",
            setup.fundraiser_pda.as_ref(),
            contributor.pubkey().as_ref(),
        ],
        &setup.program_id,
    );

    (contributor, contributor_ata, contributor_account_pda)
}

fn build_contribute_instruction(
    setup: &FundraiserSetup,
    contributor: &Address,
    contributor_ata: &Address,
    contributor_account_pda: &Address,
    amount: u64,
) -> Instruction {
    Instruction::new_with_bytes(
        setup.program_id,
        &fundraiser::instruction::Contribute { amount }.data(),
        fundraiser::accounts::ContributeAccountConstraints {
            contributor: *contributor,
            mint_to_raise: setup.mint,
            fundraiser: setup.fundraiser_pda,
            contributor_account: *contributor_account_pda,
            contributor_ata: *contributor_ata,
            vault: setup.vault,
            token_program: token_program_id(),
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    )
}

fn build_refund_instruction(
    setup: &FundraiserSetup,
    contributor: &Address,
    contributor_ata: &Address,
    contributor_account_pda: &Address,
) -> Instruction {
    Instruction::new_with_bytes(
        setup.program_id,
        &fundraiser::instruction::Refund {}.data(),
        fundraiser::accounts::RefundAccountConstraints {
            contributor: *contributor,
            maker: setup.maker.pubkey(),
            mint_to_raise: setup.mint,
            fundraiser: setup.fundraiser_pda,
            contributor_account: *contributor_account_pda,
            contributor_ata: *contributor_ata,
            vault: setup.vault,
            token_program: token_program_id(),
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    )
}

fn build_check_contributions_instruction(
    setup: &FundraiserSetup,
    maker_ata: &Address,
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
            system_program: system_program::ID,
            associated_token_program: ata_program_id(),
        }
        .to_account_metas(None),
    )
}

fn build_close_fundraiser_instruction(setup: &FundraiserSetup, maker_ata: &Address) -> Instruction {
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
            system_program: system_program::ID,
            associated_token_program: ata_program_id(),
        }
        .to_account_metas(None),
    )
}

#[test]
fn test_initialize_fundraiser() {
    let mut setup = full_setup();

    initialize_fundraiser(&mut setup, AMOUNT_TO_RAISE, DURATION_DAYS);

    let fundraiser_state = read_fundraiser_state(&setup.svm, &setup.fundraiser_pda);
    assert_eq!(fundraiser_state.amount_to_raise, AMOUNT_TO_RAISE);
    assert_eq!(fundraiser_state.current_amount, 0);
    assert_eq!(fundraiser_state.duration, DURATION_DAYS);

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
            system_program: system_program::ID,
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
        "Target below 3 major units must be rejected"
    );
    assert!(
        setup.svm.get_account(&setup.fundraiser_pda).is_none(),
        "Fundraiser account must not exist after a failed initialize"
    );
}

#[test]
fn test_contribute_inside_window_succeeds() {
    let mut setup = full_setup();
    initialize_fundraiser(&mut setup, AMOUNT_TO_RAISE, DURATION_DAYS);

    let (contributor, contributor_ata, contributor_account_pda) =
        create_funded_contributor(&mut setup);

    // One day in: well inside the 7-day window.
    warp_days_forward(&mut setup.svm, 1);

    let contribute_instruction = build_contribute_instruction(
        &setup,
        &contributor.pubkey(),
        &contributor_ata,
        &contributor_account_pda,
        MAX_CONTRIBUTION,
    );
    send_transaction_from_instructions(
        &mut setup.svm,
        vec![contribute_instruction],
        &[&contributor],
        &contributor.pubkey(),
    )
    .unwrap();

    assert_eq!(
        get_token_account_balance(&setup.svm, &setup.vault).unwrap(),
        MAX_CONTRIBUTION
    );
    assert_eq!(
        get_token_account_balance(&setup.svm, &contributor_ata).unwrap(),
        CONTRIBUTOR_STARTING_BALANCE - MAX_CONTRIBUTION
    );

    let fundraiser_state = read_fundraiser_state(&setup.svm, &setup.fundraiser_pda);
    assert_eq!(fundraiser_state.current_amount, MAX_CONTRIBUTION);

    let contributor_state = read_contributor_state(&setup.svm, &contributor_account_pda);
    assert_eq!(contributor_state.amount, MAX_CONTRIBUTION);
}

#[test]
fn test_contribute_after_deadline_fails() {
    let mut setup = full_setup();
    initialize_fundraiser(&mut setup, AMOUNT_TO_RAISE, DURATION_DAYS);

    let (contributor, contributor_ata, contributor_account_pda) =
        create_funded_contributor(&mut setup);

    // One day past the deadline.
    warp_days_forward(&mut setup.svm, DURATION_DAYS as i64 + 1);

    let contribute_instruction = build_contribute_instruction(
        &setup,
        &contributor.pubkey(),
        &contributor_ata,
        &contributor_account_pda,
        ONE_TOKEN,
    );
    let result = send_transaction_from_instructions(
        &mut setup.svm,
        vec![contribute_instruction],
        &[&contributor],
        &contributor.pubkey(),
    );
    assert!(result.is_err(), "Contributing after the deadline must fail");

    assert_eq!(
        get_token_account_balance(&setup.svm, &setup.vault).unwrap(),
        0
    );
    assert_eq!(
        get_token_account_balance(&setup.svm, &contributor_ata).unwrap(),
        CONTRIBUTOR_STARTING_BALANCE
    );
}

#[test]
fn test_contribute_below_one_major_unit_fails() {
    let mut setup = full_setup();
    initialize_fundraiser(&mut setup, AMOUNT_TO_RAISE, DURATION_DAYS);

    let (contributor, contributor_ata, contributor_account_pda) =
        create_funded_contributor(&mut setup);

    let contribute_instruction = build_contribute_instruction(
        &setup,
        &contributor.pubkey(),
        &contributor_ata,
        &contributor_account_pda,
        ONE_TOKEN - 1,
    );
    let result = send_transaction_from_instructions(
        &mut setup.svm,
        vec![contribute_instruction],
        &[&contributor],
        &contributor.pubkey(),
    );
    assert!(
        result.is_err(),
        "Contributions below one major unit must fail"
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

    let (contributor, contributor_ata, contributor_account_pda) =
        create_funded_contributor(&mut setup);

    let contribute_instruction = build_contribute_instruction(
        &setup,
        &contributor.pubkey(),
        &contributor_ata,
        &contributor_account_pda,
        ONE_TOKEN,
    );
    send_transaction_from_instructions(
        &mut setup.svm,
        vec![contribute_instruction],
        &[&contributor],
        &contributor.pubkey(),
    )
    .unwrap();

    // Still inside the window: refund must fail with FundraiserNotEnded.
    let refund_instruction = build_refund_instruction(
        &setup,
        &contributor.pubkey(),
        &contributor_ata,
        &contributor_account_pda,
    );
    let result = send_transaction_from_instructions(
        &mut setup.svm,
        vec![refund_instruction],
        &[&contributor],
        &contributor.pubkey(),
    );
    assert!(result.is_err(), "Refunding before the deadline must fail");

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

    let (contributor, contributor_ata, contributor_account_pda) =
        create_funded_contributor(&mut setup);

    let contribute_instruction = build_contribute_instruction(
        &setup,
        &contributor.pubkey(),
        &contributor_ata,
        &contributor_account_pda,
        MAX_CONTRIBUTION,
    );
    send_transaction_from_instructions(
        &mut setup.svm,
        vec![contribute_instruction],
        &[&contributor],
        &contributor.pubkey(),
    )
    .unwrap();

    // Past the deadline, target not met: refund must succeed.
    warp_days_forward(&mut setup.svm, DURATION_DAYS as i64 + 1);

    let refund_instruction = build_refund_instruction(
        &setup,
        &contributor.pubkey(),
        &contributor_ata,
        &contributor_account_pda,
    );
    send_transaction_from_instructions(
        &mut setup.svm,
        vec![refund_instruction],
        &[&contributor],
        &contributor.pubkey(),
    )
    .unwrap();

    assert_eq!(
        get_token_account_balance(&setup.svm, &setup.vault).unwrap(),
        0
    );
    assert_eq!(
        get_token_account_balance(&setup.svm, &contributor_ata).unwrap(),
        CONTRIBUTOR_STARTING_BALANCE
    );

    let fundraiser_state = read_fundraiser_state(&setup.svm, &setup.fundraiser_pda);
    assert_eq!(fundraiser_state.current_amount, 0);

    assert!(
        setup.svm.get_account(&contributor_account_pda).is_none(),
        "Contributor account must be closed after refund"
    );
}

#[test]
fn test_refund_when_target_met_fails() {
    let mut setup = full_setup();
    initialize_fundraiser(&mut setup, AMOUNT_TO_RAISE, DURATION_DAYS);

    // 10 contributors at the 10% cap reach the target exactly.
    let mut contributors = Vec::new();
    for _ in 0..10 {
        let (contributor, contributor_ata, contributor_account_pda) =
            create_funded_contributor(&mut setup);
        let contribute_instruction = build_contribute_instruction(
            &setup,
            &contributor.pubkey(),
            &contributor_ata,
            &contributor_account_pda,
            MAX_CONTRIBUTION,
        );
        send_transaction_from_instructions(
            &mut setup.svm,
            vec![contribute_instruction],
            &[&contributor],
            &contributor.pubkey(),
        )
        .unwrap();
        contributors.push((contributor, contributor_ata, contributor_account_pda));
    }

    warp_days_forward(&mut setup.svm, DURATION_DAYS as i64 + 1);

    let (contributor, contributor_ata, contributor_account_pda) = &contributors[0];
    let refund_instruction = build_refund_instruction(
        &setup,
        &contributor.pubkey(),
        contributor_ata,
        contributor_account_pda,
    );
    let result = send_transaction_from_instructions(
        &mut setup.svm,
        vec![refund_instruction],
        &[contributor],
        &contributor.pubkey(),
    );
    assert!(
        result.is_err(),
        "Refunding must fail once the target has been met"
    );
    assert_eq!(
        get_token_account_balance(&setup.svm, &setup.vault).unwrap(),
        AMOUNT_TO_RAISE
    );
}

#[test]
fn test_check_contributions_success_pays_maker_and_closes_vault() {
    let mut setup = full_setup();
    initialize_fundraiser(&mut setup, AMOUNT_TO_RAISE, DURATION_DAYS);

    // 10 contributors at the 10% cap reach the target exactly.
    for _ in 0..10 {
        let (contributor, contributor_ata, contributor_account_pda) =
            create_funded_contributor(&mut setup);
        let contribute_instruction = build_contribute_instruction(
            &setup,
            &contributor.pubkey(),
            &contributor_ata,
            &contributor_account_pda,
            MAX_CONTRIBUTION,
        );
        send_transaction_from_instructions(
            &mut setup.svm,
            vec![contribute_instruction],
            &[&contributor],
            &contributor.pubkey(),
        )
        .unwrap();
    }

    assert_eq!(
        get_token_account_balance(&setup.svm, &setup.vault).unwrap(),
        AMOUNT_TO_RAISE
    );

    let maker_ata = derive_ata(&setup.maker.pubkey(), &setup.mint);
    let check_instruction = build_check_contributions_instruction(&setup, &maker_ata);
    send_transaction_from_instructions(
        &mut setup.svm,
        vec![check_instruction],
        &[&setup.maker],
        &setup.maker.pubkey(),
    )
    .unwrap();

    assert_eq!(
        get_token_account_balance(&setup.svm, &maker_ata).unwrap(),
        AMOUNT_TO_RAISE
    );
    assert!(
        setup.svm.get_account(&setup.vault).is_none(),
        "Vault token account must be closed after a successful claim"
    );
    assert!(
        setup.svm.get_account(&setup.fundraiser_pda).is_none(),
        "Fundraiser account must be closed after a successful claim"
    );
}

#[test]
fn test_contribute_above_cap_fails() {
    let mut setup = full_setup();
    initialize_fundraiser(&mut setup, AMOUNT_TO_RAISE, DURATION_DAYS);

    let (contributor, contributor_ata, contributor_account_pda) =
        create_funded_contributor(&mut setup);

    // One minor unit over the 10% cap must fail with ContributionTooBig.
    let contribute_instruction = build_contribute_instruction(
        &setup,
        &contributor.pubkey(),
        &contributor_ata,
        &contributor_account_pda,
        MAX_CONTRIBUTION + 1,
    );
    let result = send_transaction_from_instructions(
        &mut setup.svm,
        vec![contribute_instruction],
        &[&contributor],
        &contributor.pubkey(),
    );
    assert!(
        result.is_err(),
        "A single contribution above the 10% cap must fail"
    );
    assert_eq!(
        get_token_account_balance(&setup.svm, &setup.vault).unwrap(),
        0
    );
}

#[test]
fn test_cumulative_contributions_above_cap_fail() {
    let mut setup = full_setup();
    initialize_fundraiser(&mut setup, AMOUNT_TO_RAISE, DURATION_DAYS);

    let (contributor, contributor_ata, contributor_account_pda) =
        create_funded_contributor(&mut setup);

    // Each call is under the cap on its own; the second pushes the
    // cumulative total over it and must fail with
    // MaximumContributionsReached.
    let first_contribution = 2 * ONE_TOKEN;
    let second_contribution = MAX_CONTRIBUTION - ONE_TOKEN;

    let first_instruction = build_contribute_instruction(
        &setup,
        &contributor.pubkey(),
        &contributor_ata,
        &contributor_account_pda,
        first_contribution,
    );
    send_transaction_from_instructions(
        &mut setup.svm,
        vec![first_instruction],
        &[&contributor],
        &contributor.pubkey(),
    )
    .unwrap();

    let second_instruction = build_contribute_instruction(
        &setup,
        &contributor.pubkey(),
        &contributor_ata,
        &contributor_account_pda,
        second_contribution,
    );
    let result = send_transaction_from_instructions(
        &mut setup.svm,
        vec![second_instruction],
        &[&contributor],
        &contributor.pubkey(),
    );
    assert!(
        result.is_err(),
        "Contributions that cumulatively exceed the 10% cap must fail"
    );

    assert_eq!(
        get_token_account_balance(&setup.svm, &setup.vault).unwrap(),
        first_contribution
    );
    let contributor_state = read_contributor_state(&setup.svm, &contributor_account_pda);
    assert_eq!(contributor_state.amount, first_contribution);
}

#[test]
fn test_close_fundraiser_after_failed_raise_allows_a_new_raise() {
    let mut setup = full_setup();
    initialize_fundraiser(&mut setup, AMOUNT_TO_RAISE, DURATION_DAYS);

    let (contributor, contributor_ata, contributor_account_pda) =
        create_funded_contributor(&mut setup);
    let contribute_instruction = build_contribute_instruction(
        &setup,
        &contributor.pubkey(),
        &contributor_ata,
        &contributor_account_pda,
        MAX_CONTRIBUTION,
    );
    send_transaction_from_instructions(
        &mut setup.svm,
        vec![contribute_instruction],
        &[&contributor],
        &contributor.pubkey(),
    )
    .unwrap();

    // The raise fails; the contributor takes their refund.
    warp_days_forward(&mut setup.svm, DURATION_DAYS as i64 + 1);
    let refund_instruction = build_refund_instruction(
        &setup,
        &contributor.pubkey(),
        &contributor_ata,
        &contributor_account_pda,
    );
    send_transaction_from_instructions(
        &mut setup.svm,
        vec![refund_instruction],
        &[&contributor],
        &contributor.pubkey(),
    )
    .unwrap();

    // The maker retires the failed fundraiser.
    let maker_ata = derive_ata(&setup.maker.pubkey(), &setup.mint);
    let close_instruction = build_close_fundraiser_instruction(&setup, &maker_ata);
    send_transaction_from_instructions(
        &mut setup.svm,
        vec![close_instruction],
        &[&setup.maker],
        &setup.maker.pubkey(),
    )
    .unwrap();

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

    let maker_ata = derive_ata(&setup.maker.pubkey(), &setup.mint);
    let close_instruction = build_close_fundraiser_instruction(&setup, &maker_ata);
    let result = send_transaction_from_instructions(
        &mut setup.svm,
        vec![close_instruction],
        &[&setup.maker],
        &setup.maker.pubkey(),
    );
    assert!(
        result.is_err(),
        "Closing a fundraiser before its deadline must fail"
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

    let (contributor, contributor_ata, contributor_account_pda) =
        create_funded_contributor(&mut setup);
    let contribute_instruction = build_contribute_instruction(
        &setup,
        &contributor.pubkey(),
        &contributor_ata,
        &contributor_account_pda,
        MAX_CONTRIBUTION,
    );
    send_transaction_from_instructions(
        &mut setup.svm,
        vec![contribute_instruction],
        &[&contributor],
        &contributor.pubkey(),
    )
    .unwrap();

    // Past the deadline but the contribution has not been refunded, so
    // closing would strand it in the vault.
    warp_days_forward(&mut setup.svm, DURATION_DAYS as i64 + 1);

    let maker_ata = derive_ata(&setup.maker.pubkey(), &setup.mint);
    let close_instruction = build_close_fundraiser_instruction(&setup, &maker_ata);
    let result = send_transaction_from_instructions(
        &mut setup.svm,
        vec![close_instruction],
        &[&setup.maker],
        &setup.maker.pubkey(),
    );
    assert!(
        result.is_err(),
        "Closing must fail while contributions remain unrefunded"
    );
    assert_eq!(
        get_token_account_balance(&setup.svm, &setup.vault).unwrap(),
        MAX_CONTRIBUTION
    );
}

#[test]
fn test_close_fundraiser_when_target_met_fails() {
    let mut setup = full_setup();
    initialize_fundraiser(&mut setup, AMOUNT_TO_RAISE, DURATION_DAYS);

    // 10 contributors at the 10% cap reach the target exactly.
    for _ in 0..10 {
        let (contributor, contributor_ata, contributor_account_pda) =
            create_funded_contributor(&mut setup);
        let contribute_instruction = build_contribute_instruction(
            &setup,
            &contributor.pubkey(),
            &contributor_ata,
            &contributor_account_pda,
            MAX_CONTRIBUTION,
        );
        send_transaction_from_instructions(
            &mut setup.svm,
            vec![contribute_instruction],
            &[&contributor],
            &contributor.pubkey(),
        )
        .unwrap();
    }

    warp_days_forward(&mut setup.svm, DURATION_DAYS as i64 + 1);

    // A successful raise exits through check_contributions, never close.
    let maker_ata = derive_ata(&setup.maker.pubkey(), &setup.mint);
    let close_instruction = build_close_fundraiser_instruction(&setup, &maker_ata);
    let result = send_transaction_from_instructions(
        &mut setup.svm,
        vec![close_instruction],
        &[&setup.maker],
        &setup.maker.pubkey(),
    );
    assert!(
        result.is_err(),
        "Closing must fail when the target was met; the claim is the exit"
    );
    assert_eq!(
        get_token_account_balance(&setup.svm, &setup.vault).unwrap(),
        AMOUNT_TO_RAISE
    );
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

    warp_days_forward(&mut setup.svm, DURATION_DAYS as i64 + 1);

    let maker_ata = derive_ata(&setup.maker.pubkey(), &setup.mint);
    let close_instruction = build_close_fundraiser_instruction(&setup, &maker_ata);
    send_transaction_from_instructions(
        &mut setup.svm,
        vec![close_instruction],
        &[&setup.maker],
        &setup.maker.pubkey(),
    )
    .unwrap();

    assert_eq!(
        get_token_account_balance(&setup.svm, &maker_ata).unwrap(),
        donation
    );
    assert!(setup.svm.get_account(&setup.fundraiser_pda).is_none());
    assert!(setup.svm.get_account(&setup.vault).is_none());
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

    let maker_ata = derive_ata(&setup.maker.pubkey(), &setup.mint);
    let check_instruction = build_check_contributions_instruction(&setup, &maker_ata);
    let result = send_transaction_from_instructions(
        &mut setup.svm,
        vec![check_instruction],
        &[&setup.maker],
        &setup.maker.pubkey(),
    );
    assert!(
        result.is_err(),
        "Direct donations to the vault must not unlock the claim"
    );
    assert!(
        setup.svm.get_account(&setup.fundraiser_pda).is_some(),
        "Fundraiser account must stay open after a failed claim"
    );
}
