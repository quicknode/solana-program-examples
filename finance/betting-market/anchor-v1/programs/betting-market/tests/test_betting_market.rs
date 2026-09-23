use {
    anchor_lang::{
        prelude::Clock,
        solana_program::{instruction::Instruction, pubkey::Pubkey, system_program},
        AccountDeserialize, InstructionData, ToAccountMetas,
    },
    betting_market::{error::BettingError, User, MAX_BETS_PER_USER},
    litesvm::LiteSVM,
    solana_keypair::Keypair,
    solana_kite::{
        create_associated_token_account, create_token_mint, create_wallet,
        get_token_account_balance, mint_tokens_to_token_account, send_transaction_from_instructions,
        SolanaKiteError,
    },
    solana_signer::Signer,
};

const DECIMALS: u8 = 6;
const FEE_BPS: u16 = 200; // 2%

// A fixed unix timestamp the clock is warped to before anything is written,
// so every market's betting window is deterministic: it closes a week later.
const START_TIME: i64 = 1_750_000_000;
const SECONDS_PER_DAY: i64 = 24 * 60 * 60;
const BETTING_CLOSES_AT: i64 = START_TIME + 7 * SECONDS_PER_DAY;

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
    Pubkey::find_program_address(
        &[wallet.as_ref(), token_program_id().as_ref(), mint.as_ref()],
        &ata_program_id(),
    )
    .0
}

fn config_pda() -> Pubkey {
    Pubkey::find_program_address(&[b"config"], &betting_market::id()).0
}

fn event_pda(event_id: u64) -> Pubkey {
    Pubkey::find_program_address(&[b"event", &event_id.to_le_bytes()], &betting_market::id()).0
}

fn outcome_pda(event: &Pubkey, index: u8) -> Pubkey {
    Pubkey::find_program_address(&[b"outcome", event.as_ref(), &[index]], &betting_market::id()).0
}

fn bet_pda(outcome: &Pubkey, bettor: &Pubkey) -> Pubkey {
    Pubkey::find_program_address(&[b"bet", outcome.as_ref(), bettor.as_ref()], &betting_market::id())
        .0
}

fn user_pda(bettor: &Pubkey) -> Pubkey {
    Pubkey::find_program_address(&[b"user", bettor.as_ref()], &betting_market::id()).0
}

struct Market {
    svm: LiteSVM,
    admin: Keypair,
    mint: Pubkey,
    fee_recipient: Keypair,
    fee_recipient_ata: Pubkey,
}

// Spin up the SVM with the program loaded, an admin wallet, the stake-token mint
// (admin is the mint authority), and a fee-recipient wallet with an ATA.
fn setup() -> Market {
    let mut svm = LiteSVM::new();
    warp_to(&mut svm, START_TIME);
    let program_bytes = include_bytes!("../../../target/deploy/betting_market.so");
    svm.add_program(betting_market::id(), program_bytes).unwrap();

    let admin = create_wallet(&mut svm, 100_000_000_000).unwrap();
    let mint = create_token_mint(&mut svm, &admin, DECIMALS, None).unwrap();

    let fee_recipient = create_wallet(&mut svm, 10_000_000_000).unwrap();
    let fee_recipient_ata =
        create_associated_token_account(&mut svm, &fee_recipient.pubkey(), &mint, &admin).unwrap();

    Market {
        svm,
        admin,
        mint,
        fee_recipient,
        fee_recipient_ata,
    }
}

// Move the clock to `unix_timestamp`. Also expires the blockhash, so a retried
// instruction after the warp is not dropped as a duplicate.
fn warp_to(svm: &mut LiteSVM, unix_timestamp: i64) {
    let mut clock: Clock = svm.get_sysvar();
    clock.unix_timestamp = unix_timestamp;
    svm.set_sysvar(&clock);
    svm.expire_blockhash();
}

// Anchor numbers a program's errors from 6000 in declaration order, and a
// failed transaction reports the number as `Custom(n)`. Matching it proves the
// transaction failed for the rule under test, not for some unrelated reason.
fn assert_fails_with(result: Result<(), SolanaKiteError>, expected: BettingError) {
    let code = 6000 + expected as u32;
    let error = format!("{:?}", result.expect_err("transaction should have failed"));
    assert!(
        error.contains(&format!("Custom({code})")),
        "expected error {code}, got: {error}"
    );
}

// Create a funded bettor with a token ATA holding `amount` of the stake token.
fn create_bettor(market: &mut Market, amount: u64) -> (Keypair, Pubkey) {
    let bettor = create_wallet(&mut market.svm, 10_000_000_000).unwrap();
    let ata = create_associated_token_account(
        &mut market.svm,
        &bettor.pubkey(),
        &market.mint,
        &market.admin,
    )
    .unwrap();
    mint_tokens_to_token_account(&mut market.svm, &market.mint, &ata, amount, &market.admin)
        .unwrap();
    (bettor, ata)
}

fn initialize_config_ix(admin: Pubkey, mint: Pubkey, fee_recipient: Pubkey) -> Instruction {
    Instruction::new_with_bytes(
        betting_market::id(),
        &betting_market::instruction::InitializeConfig {
            default_fee_bps: FEE_BPS,
            fee_recipient,
        }
        .data(),
        betting_market::accounts::InitializeConfigAccountConstraints {
            admin,
            token_mint: mint,
            config: config_pda(),
            token_program: token_program_id(),
            system_program: system_program::id(),
        }
        .to_account_metas(None),
    )
}

fn initialize_event_ix(admin: Pubkey, mint: Pubkey, event_id: u64, description: &str) -> Instruction {
    initialize_event_closing_at_ix(admin, mint, event_id, description, BETTING_CLOSES_AT)
}

fn initialize_event_closing_at_ix(
    admin: Pubkey,
    mint: Pubkey,
    event_id: u64,
    description: &str,
    betting_closes_at: i64,
) -> Instruction {
    let event = event_pda(event_id);
    Instruction::new_with_bytes(
        betting_market::id(),
        &betting_market::instruction::InitializeEvent {
            event_id,
            betting_closes_at,
            description: description.to_string(),
        }
        .data(),
        betting_market::accounts::InitializeEventAccountConstraints {
            admin,
            config: config_pda(),
            token_mint: mint,
            event,
            vault: derive_ata(&event, &mint),
            associated_token_program: ata_program_id(),
            token_program: token_program_id(),
            system_program: system_program::id(),
        }
        .to_account_metas(None),
    )
}

fn add_outcome_ix(admin: Pubkey, event_id: u64, index: u8, label: &str) -> Instruction {
    let event = event_pda(event_id);
    Instruction::new_with_bytes(
        betting_market::id(),
        &betting_market::instruction::AddOutcome {
            label: label.to_string(),
        }
        .data(),
        betting_market::accounts::AddOutcomeAccountConstraints {
            admin,
            config: config_pda(),
            event,
            outcome: outcome_pda(&event, index),
            system_program: system_program::id(),
        }
        .to_account_metas(None),
    )
}

fn open_betting_ix(admin: Pubkey, event_id: u64) -> Instruction {
    Instruction::new_with_bytes(
        betting_market::id(),
        &betting_market::instruction::OpenBetting {}.data(),
        betting_market::accounts::OpenBettingAccountConstraints {
            admin,
            config: config_pda(),
            event: event_pda(event_id),
        }
        .to_account_metas(None),
    )
}

fn place_bet_ix(
    mint: Pubkey,
    bettor: &Pubkey,
    bettor_ata: &Pubkey,
    event_id: u64,
    outcome_index: u8,
    amount: u64,
) -> Instruction {
    let event = event_pda(event_id);
    let outcome = outcome_pda(&event, outcome_index);
    Instruction::new_with_bytes(
        betting_market::id(),
        &betting_market::instruction::PlaceBet { amount }.data(),
        betting_market::accounts::PlaceBetAccountConstraints {
            bettor: *bettor,
            config: config_pda(),
            token_mint: mint,
            event,
            outcome,
            bettor_token_account: *bettor_ata,
            vault: derive_ata(&event, &mint),
            bet: bet_pda(&outcome, bettor),
            user: user_pda(bettor),
            associated_token_program: ata_program_id(),
            token_program: token_program_id(),
            system_program: system_program::id(),
        }
        .to_account_metas(None),
    )
}

fn settle_event_ix(
    admin: Pubkey,
    mint: Pubkey,
    fee_recipient: Pubkey,
    fee_recipient_ata: Pubkey,
    event_id: u64,
    winning_outcome_index: u8,
) -> Instruction {
    let event = event_pda(event_id);
    Instruction::new_with_bytes(
        betting_market::id(),
        &betting_market::instruction::SettleEvent {
            winning_outcome_index,
        }
        .data(),
        betting_market::accounts::SettleEventAccountConstraints {
            admin,
            config: config_pda(),
            token_mint: mint,
            event,
            winning_outcome: outcome_pda(&event, winning_outcome_index),
            vault: derive_ata(&event, &mint),
            fee_recipient,
            fee_recipient_token_account: fee_recipient_ata,
            associated_token_program: ata_program_id(),
            token_program: token_program_id(),
            system_program: system_program::id(),
        }
        .to_account_metas(None),
    )
}

fn claim_winnings_ix(
    mint: Pubkey,
    bettor: &Pubkey,
    bettor_ata: &Pubkey,
    event_id: u64,
    outcome_index: u8,
) -> Instruction {
    let event = event_pda(event_id);
    let outcome = outcome_pda(&event, outcome_index);
    Instruction::new_with_bytes(
        betting_market::id(),
        &betting_market::instruction::ClaimWinnings {}.data(),
        betting_market::accounts::ClaimWinningsAccountConstraints {
            bettor: *bettor,
            token_mint: mint,
            event,
            bet: bet_pda(&outcome, bettor),
            user: user_pda(bettor),
            bettor_token_account: *bettor_ata,
            vault: derive_ata(&event, &mint),
            token_program: token_program_id(),
        }
        .to_account_metas(None),
    )
}

fn cancel_event_ix(admin: Pubkey, event_id: u64) -> Instruction {
    Instruction::new_with_bytes(
        betting_market::id(),
        &betting_market::instruction::CancelEvent {}.data(),
        betting_market::accounts::CancelEventAccountConstraints {
            admin,
            config: config_pda(),
            event: event_pda(event_id),
        }
        .to_account_metas(None),
    )
}

fn claim_refund_ix(
    mint: Pubkey,
    bettor: &Pubkey,
    bettor_ata: &Pubkey,
    event_id: u64,
    outcome_index: u8,
) -> Instruction {
    let event = event_pda(event_id);
    let outcome = outcome_pda(&event, outcome_index);
    Instruction::new_with_bytes(
        betting_market::id(),
        &betting_market::instruction::ClaimRefund {}.data(),
        betting_market::accounts::ClaimRefundAccountConstraints {
            bettor: *bettor,
            token_mint: mint,
            event,
            bet: bet_pda(&outcome, bettor),
            user: user_pda(bettor),
            bettor_token_account: *bettor_ata,
            vault: derive_ata(&event, &mint),
            token_program: token_program_id(),
        }
        .to_account_metas(None),
    )
}

fn close_losing_bet_ix(bettor: &Pubkey, event_id: u64, outcome_index: u8) -> Instruction {
    let event = event_pda(event_id);
    let outcome = outcome_pda(&event, outcome_index);
    Instruction::new_with_bytes(
        betting_market::id(),
        &betting_market::instruction::CloseLosingBet {}.data(),
        betting_market::accounts::CloseLosingBetAccountConstraints {
            bettor: *bettor,
            event,
            bet: bet_pda(&outcome, bettor),
            user: user_pda(bettor),
        }
        .to_account_metas(None),
    )
}

// Decode a User account so tests can assert exactly which Bet addresses the
// per-wallet index currently holds.
fn read_user_bets(market: &Market, bettor: &Pubkey) -> Vec<Pubkey> {
    let account = market.svm.get_account(&user_pda(bettor)).unwrap();
    User::try_deserialize(&mut account.data.as_slice())
        .unwrap()
        .bets
}

fn init_config(market: &mut Market) {
    let admin = market.admin.pubkey();
    let mint = market.mint;
    let fee_recipient = market.fee_recipient.pubkey();
    send_transaction_from_instructions(
        &mut market.svm,
        vec![initialize_config_ix(admin, mint, fee_recipient)],
        &[&market.admin],
        &admin,
    )
    .unwrap();
}

#[test]
fn test_full_lifecycle() {
    let mut market = setup();
    let event_id: u64 = 1;

    // Stakes chosen so the pro-rata split divides evenly (no dust):
    // Yes pool 400 (Alice 100 + Bob 300), No pool 200 (Carol). Yes wins.
    let (alice, alice_ata) = create_bettor(&mut market, 1_000);
    let (bob, bob_ata) = create_bettor(&mut market, 1_000);
    let (carol, carol_ata) = create_bettor(&mut market, 1_000);

    init_config(&mut market);

    let admin = market.admin.pubkey();
    let mint = market.mint;
    send_transaction_from_instructions(
        &mut market.svm,
        vec![
            initialize_event_ix(admin, mint, event_id, "Will it rain tomorrow?"),
            add_outcome_ix(admin, event_id, 0, "Yes"),
            add_outcome_ix(admin, event_id, 1, "No"),
            open_betting_ix(admin, event_id),
        ],
        &[&market.admin],
        &admin,
    )
    .unwrap();

    send_transaction_from_instructions(
        &mut market.svm,
        vec![place_bet_ix(mint, &alice.pubkey(), &alice_ata, event_id, 0, 100)],
        &[&alice],
        &alice.pubkey(),
    )
    .unwrap();
    send_transaction_from_instructions(
        &mut market.svm,
        vec![place_bet_ix(mint, &bob.pubkey(), &bob_ata, event_id, 0, 300)],
        &[&bob],
        &bob.pubkey(),
    )
    .unwrap();
    send_transaction_from_instructions(
        &mut market.svm,
        vec![place_bet_ix(mint, &carol.pubkey(), &carol_ata, event_id, 1, 200)],
        &[&carol],
        &carol.pubkey(),
    )
    .unwrap();

    // Vault holds the entire pool.
    let vault = derive_ata(&event_pda(event_id), &mint);
    assert_eq!(get_token_account_balance(&market.svm, &vault).unwrap(), 600);
    assert_eq!(
        read_user_bets(&market, &alice.pubkey()),
        vec![bet_pda(&outcome_pda(&event_pda(event_id), 0), &alice.pubkey())]
    );

    // Settle to "Yes" (index 0). Losing pool 200, fee = 2% = 4, distributable = 196.
    let fee_recipient = market.fee_recipient.pubkey();
    let fee_recipient_ata = market.fee_recipient_ata;
    // Betting has closed, so the event can be settled.
    warp_to(&mut market.svm, BETTING_CLOSES_AT);
    send_transaction_from_instructions(
        &mut market.svm,
        vec![settle_event_ix(admin, mint, fee_recipient, fee_recipient_ata, event_id, 0)],
        &[&market.admin],
        &admin,
    )
    .unwrap();
    assert_eq!(
        get_token_account_balance(&market.svm, &fee_recipient_ata).unwrap(),
        4
    );

    // Alice: 100 + 100*196/400 = 149. Bob: 300 + 300*196/400 = 447.
    send_transaction_from_instructions(
        &mut market.svm,
        vec![claim_winnings_ix(mint, &alice.pubkey(), &alice_ata, event_id, 0)],
        &[&alice],
        &alice.pubkey(),
    )
    .unwrap();
    send_transaction_from_instructions(
        &mut market.svm,
        vec![claim_winnings_ix(mint, &bob.pubkey(), &bob_ata, event_id, 0)],
        &[&bob],
        &bob.pubkey(),
    )
    .unwrap();

    assert_eq!(
        get_token_account_balance(&market.svm, &alice_ata).unwrap(),
        1_000 - 100 + 149
    );
    assert_eq!(
        get_token_account_balance(&market.svm, &bob_ata).unwrap(),
        1_000 - 300 + 447
    );
    // Pool fully distributed: 400 stakes + 196 winnings + 4 fee = 600.
    assert_eq!(get_token_account_balance(&market.svm, &vault).unwrap(), 0);

    // Claiming closed the winners' Bet accounts and emptied their indexes.
    assert!(read_user_bets(&market, &alice.pubkey()).is_empty());
    assert!(read_user_bets(&market, &bob.pubkey()).is_empty());

    // Carol bet the losing outcome, so she has nothing to claim.
    let carol_claim = send_transaction_from_instructions(
        &mut market.svm,
        vec![claim_winnings_ix(mint, &carol.pubkey(), &carol_ata, event_id, 1)],
        &[&carol],
        &carol.pubkey(),
    );
    assert!(carol_claim.is_err(), "loser must not be able to claim winnings");

    // Her losing position stays in the index until she closes it.
    let carol_bet = bet_pda(&outcome_pda(&event_pda(event_id), 1), &carol.pubkey());
    assert_eq!(read_user_bets(&market, &carol.pubkey()), vec![carol_bet]);
    send_transaction_from_instructions(
        &mut market.svm,
        vec![close_losing_bet_ix(&carol.pubkey(), event_id, 1)],
        &[&carol],
        &carol.pubkey(),
    )
    .unwrap();
    assert!(read_user_bets(&market, &carol.pubkey()).is_empty());
}

#[test]
fn test_only_admin_can_initialize_event() {
    let mut market = setup();
    init_config(&mut market);

    let mint = market.mint;
    let mallory = create_wallet(&mut market.svm, 10_000_000_000).unwrap();
    let result = send_transaction_from_instructions(
        &mut market.svm,
        vec![initialize_event_ix(mallory.pubkey(), mint, 7, "Unauthorized event")],
        &[&mallory],
        &mallory.pubkey(),
    );
    assert!(result.is_err(), "non-admin must not create an event");
}

#[test]
fn test_cannot_bet_after_settle() {
    let mut market = setup();
    let event_id: u64 = 2;
    let (alice, alice_ata) = create_bettor(&mut market, 1_000);
    let (bob, bob_ata) = create_bettor(&mut market, 1_000);

    init_config(&mut market);
    let admin = market.admin.pubkey();
    let mint = market.mint;
    let fee_recipient = market.fee_recipient.pubkey();
    let fee_recipient_ata = market.fee_recipient_ata;
    send_transaction_from_instructions(
        &mut market.svm,
        vec![
            initialize_event_ix(admin, mint, event_id, "Coin flip"),
            add_outcome_ix(admin, event_id, 0, "Heads"),
            add_outcome_ix(admin, event_id, 1, "Tails"),
            open_betting_ix(admin, event_id),
        ],
        &[&market.admin],
        &admin,
    )
    .unwrap();
    send_transaction_from_instructions(
        &mut market.svm,
        vec![place_bet_ix(mint, &alice.pubkey(), &alice_ata, event_id, 0, 100)],
        &[&alice],
        &alice.pubkey(),
    )
    .unwrap();
    // Betting has closed, so the event can be settled.
    warp_to(&mut market.svm, BETTING_CLOSES_AT);
    send_transaction_from_instructions(
        &mut market.svm,
        vec![settle_event_ix(admin, mint, fee_recipient, fee_recipient_ata, event_id, 0)],
        &[&market.admin],
        &admin,
    )
    .unwrap();

    let late_bet = send_transaction_from_instructions(
        &mut market.svm,
        vec![place_bet_ix(mint, &bob.pubkey(), &bob_ata, event_id, 1, 100)],
        &[&bob],
        &bob.pubkey(),
    );
    assert_fails_with(late_bet, BettingError::EventNotOpen);
}

#[test]
fn test_double_claim_fails() {
    let mut market = setup();
    let event_id: u64 = 3;
    let (alice, alice_ata) = create_bettor(&mut market, 1_000);
    let (carol, carol_ata) = create_bettor(&mut market, 1_000);

    init_config(&mut market);
    let admin = market.admin.pubkey();
    let mint = market.mint;
    let fee_recipient = market.fee_recipient.pubkey();
    let fee_recipient_ata = market.fee_recipient_ata;
    send_transaction_from_instructions(
        &mut market.svm,
        vec![
            initialize_event_ix(admin, mint, event_id, "Match winner"),
            add_outcome_ix(admin, event_id, 0, "Home"),
            add_outcome_ix(admin, event_id, 1, "Away"),
            open_betting_ix(admin, event_id),
        ],
        &[&market.admin],
        &admin,
    )
    .unwrap();
    send_transaction_from_instructions(
        &mut market.svm,
        vec![place_bet_ix(mint, &alice.pubkey(), &alice_ata, event_id, 0, 100)],
        &[&alice],
        &alice.pubkey(),
    )
    .unwrap();
    send_transaction_from_instructions(
        &mut market.svm,
        vec![place_bet_ix(mint, &carol.pubkey(), &carol_ata, event_id, 1, 100)],
        &[&carol],
        &carol.pubkey(),
    )
    .unwrap();
    // Betting has closed, so the event can be settled.
    warp_to(&mut market.svm, BETTING_CLOSES_AT);
    send_transaction_from_instructions(
        &mut market.svm,
        vec![settle_event_ix(admin, mint, fee_recipient, fee_recipient_ata, event_id, 0)],
        &[&market.admin],
        &admin,
    )
    .unwrap();

    send_transaction_from_instructions(
        &mut market.svm,
        vec![claim_winnings_ix(mint, &alice.pubkey(), &alice_ata, event_id, 0)],
        &[&alice],
        &alice.pubkey(),
    )
    .unwrap();
    // A fresh blockhash so the second claim is a distinct transaction.
    market.svm.expire_blockhash();
    let second = send_transaction_from_instructions(
        &mut market.svm,
        vec![claim_winnings_ix(mint, &alice.pubkey(), &alice_ata, event_id, 0)],
        &[&alice],
        &alice.pubkey(),
    );
    assert!(second.is_err(), "claiming the same bet twice must fail");
}

#[test]
fn test_settle_outcome_without_bets_fails() {
    let mut market = setup();
    let event_id: u64 = 4;
    let (alice, alice_ata) = create_bettor(&mut market, 1_000);

    init_config(&mut market);
    let admin = market.admin.pubkey();
    let mint = market.mint;
    let fee_recipient = market.fee_recipient.pubkey();
    let fee_recipient_ata = market.fee_recipient_ata;
    send_transaction_from_instructions(
        &mut market.svm,
        vec![
            initialize_event_ix(admin, mint, event_id, "Two horse race"),
            add_outcome_ix(admin, event_id, 0, "Horse A"),
            add_outcome_ix(admin, event_id, 1, "Horse B"),
            open_betting_ix(admin, event_id),
        ],
        &[&market.admin],
        &admin,
    )
    .unwrap();
    // Everyone bets Horse A; Horse B has no bets.
    send_transaction_from_instructions(
        &mut market.svm,
        vec![place_bet_ix(mint, &alice.pubkey(), &alice_ata, event_id, 0, 100)],
        &[&alice],
        &alice.pubkey(),
    )
    .unwrap();

    // Betting has closed, so the event can be settled.
    warp_to(&mut market.svm, BETTING_CLOSES_AT);
    let result = send_transaction_from_instructions(
        &mut market.svm,
        vec![settle_event_ix(admin, mint, fee_recipient, fee_recipient_ata, event_id, 1)],
        &[&market.admin],
        &admin,
    );
    assert_fails_with(result, BettingError::OutcomeHasNoBets);
}

#[test]
fn test_cancel_and_refund() {
    let mut market = setup();
    let event_id: u64 = 5;
    let (alice, alice_ata) = create_bettor(&mut market, 1_000);
    let (carol, carol_ata) = create_bettor(&mut market, 1_000);

    init_config(&mut market);
    let admin = market.admin.pubkey();
    let mint = market.mint;
    send_transaction_from_instructions(
        &mut market.svm,
        vec![
            initialize_event_ix(admin, mint, event_id, "Voided event"),
            add_outcome_ix(admin, event_id, 0, "A"),
            add_outcome_ix(admin, event_id, 1, "B"),
            open_betting_ix(admin, event_id),
        ],
        &[&market.admin],
        &admin,
    )
    .unwrap();
    send_transaction_from_instructions(
        &mut market.svm,
        vec![place_bet_ix(mint, &alice.pubkey(), &alice_ata, event_id, 0, 250)],
        &[&alice],
        &alice.pubkey(),
    )
    .unwrap();
    send_transaction_from_instructions(
        &mut market.svm,
        vec![place_bet_ix(mint, &carol.pubkey(), &carol_ata, event_id, 1, 750)],
        &[&carol],
        &carol.pubkey(),
    )
    .unwrap();

    send_transaction_from_instructions(
        &mut market.svm,
        vec![cancel_event_ix(admin, event_id)],
        &[&market.admin],
        &admin,
    )
    .unwrap();

    let alice_bet = bet_pda(&outcome_pda(&event_pda(event_id), 0), &alice.pubkey());
    assert_eq!(read_user_bets(&market, &alice.pubkey()), vec![alice_bet]);

    send_transaction_from_instructions(
        &mut market.svm,
        vec![claim_refund_ix(mint, &alice.pubkey(), &alice_ata, event_id, 0)],
        &[&alice],
        &alice.pubkey(),
    )
    .unwrap();

    // The refund closed Alice's Bet account and removed it from her index.
    assert!(read_user_bets(&market, &alice.pubkey()).is_empty());

    send_transaction_from_instructions(
        &mut market.svm,
        vec![claim_refund_ix(mint, &carol.pubkey(), &carol_ata, event_id, 1)],
        &[&carol],
        &carol.pubkey(),
    )
    .unwrap();

    // Both bettors made whole; no fee on a cancelled event.
    assert_eq!(get_token_account_balance(&market.svm, &alice_ata).unwrap(), 1_000);
    assert_eq!(get_token_account_balance(&market.svm, &carol_ata).unwrap(), 1_000);
    let vault = derive_ata(&event_pda(event_id), &mint);
    assert_eq!(get_token_account_balance(&market.svm, &vault).unwrap(), 0);
}

#[test]
fn test_close_losing_bet_only_after_settle_and_only_for_losers() {
    let mut market = setup();
    let event_id: u64 = 6;
    let (alice, alice_ata) = create_bettor(&mut market, 1_000);
    let (carol, carol_ata) = create_bettor(&mut market, 1_000);

    init_config(&mut market);
    let admin = market.admin.pubkey();
    let mint = market.mint;
    let fee_recipient = market.fee_recipient.pubkey();
    let fee_recipient_ata = market.fee_recipient_ata;
    send_transaction_from_instructions(
        &mut market.svm,
        vec![
            initialize_event_ix(admin, mint, event_id, "Derby winner"),
            add_outcome_ix(admin, event_id, 0, "Red"),
            add_outcome_ix(admin, event_id, 1, "Blue"),
            open_betting_ix(admin, event_id),
        ],
        &[&market.admin],
        &admin,
    )
    .unwrap();
    send_transaction_from_instructions(
        &mut market.svm,
        vec![place_bet_ix(mint, &alice.pubkey(), &alice_ata, event_id, 0, 100)],
        &[&alice],
        &alice.pubkey(),
    )
    .unwrap();
    send_transaction_from_instructions(
        &mut market.svm,
        vec![place_bet_ix(mint, &carol.pubkey(), &carol_ata, event_id, 1, 100)],
        &[&carol],
        &carol.pubkey(),
    )
    .unwrap();

    // The event is still open, so no position is a losing one yet.
    let premature_close = send_transaction_from_instructions(
        &mut market.svm,
        vec![close_losing_bet_ix(&carol.pubkey(), event_id, 1)],
        &[&carol],
        &carol.pubkey(),
    );
    assert!(premature_close.is_err(), "closing before settlement must fail");

    // Betting has closed, so the event can be settled.
    warp_to(&mut market.svm, BETTING_CLOSES_AT);
    send_transaction_from_instructions(
        &mut market.svm,
        vec![settle_event_ix(admin, mint, fee_recipient, fee_recipient_ata, event_id, 0)],
        &[&market.admin],
        &admin,
    )
    .unwrap();

    // Alice won; her bet must be closed via claim_winnings, not discarded.
    let winner_close = send_transaction_from_instructions(
        &mut market.svm,
        vec![close_losing_bet_ix(&alice.pubkey(), event_id, 0)],
        &[&alice],
        &alice.pubkey(),
    );
    assert!(winner_close.is_err(), "a winning bet must not be closed as losing");
    let alice_bet = bet_pda(&outcome_pda(&event_pda(event_id), 0), &alice.pubkey());
    assert_eq!(read_user_bets(&market, &alice.pubkey()), vec![alice_bet]);

    // Carol lost; closing frees her index slot. A fresh blockhash so this is
    // a distinct transaction from her premature attempt above.
    market.svm.expire_blockhash();
    send_transaction_from_instructions(
        &mut market.svm,
        vec![close_losing_bet_ix(&carol.pubkey(), event_id, 1)],
        &[&carol],
        &carol.pubkey(),
    )
    .unwrap();
    assert!(read_user_bets(&market, &carol.pubkey()).is_empty());
}

// Regression test: closing a Bet must free its User index slot, so a wallet
// that fills all MAX_BETS_PER_USER slots can bet again after unwinding a
// position. Without the removal, a full index rejects every future bet on
// every market, permanently.
#[test]
fn test_closing_a_bet_frees_a_slot_for_a_new_bet() {
    const STAKE: u64 = 10;
    let mut market = setup();
    let full_event_id: u64 = 7;
    let second_event_id: u64 = 8;
    // Enough outcomes to fill the index and attempt one more bet.
    let outcome_count = (MAX_BETS_PER_USER + 1) as u8;
    let (alice, alice_ata) = create_bettor(&mut market, outcome_count as u64 * STAKE);

    init_config(&mut market);
    let admin = market.admin.pubkey();
    let mint = market.mint;

    send_transaction_from_instructions(
        &mut market.svm,
        vec![initialize_event_ix(admin, mint, full_event_id, "Wide field")],
        &[&market.admin],
        &admin,
    )
    .unwrap();
    for index in 0..outcome_count {
        send_transaction_from_instructions(
            &mut market.svm,
            vec![add_outcome_ix(admin, full_event_id, index, &format!("Runner {index}"))],
            &[&market.admin],
            &admin,
        )
        .unwrap();
    }
    send_transaction_from_instructions(
        &mut market.svm,
        vec![open_betting_ix(admin, full_event_id)],
        &[&market.admin],
        &admin,
    )
    .unwrap();
    send_transaction_from_instructions(
        &mut market.svm,
        vec![
            initialize_event_ix(admin, mint, second_event_id, "Second market"),
            add_outcome_ix(admin, second_event_id, 0, "Yes"),
            add_outcome_ix(admin, second_event_id, 1, "No"),
            open_betting_ix(admin, second_event_id),
        ],
        &[&market.admin],
        &admin,
    )
    .unwrap();

    // Fill every slot in Alice's index.
    for index in 0..MAX_BETS_PER_USER as u8 {
        send_transaction_from_instructions(
            &mut market.svm,
            vec![place_bet_ix(mint, &alice.pubkey(), &alice_ata, full_event_id, index, STAKE)],
            &[&alice],
            &alice.pubkey(),
        )
        .unwrap();
    }
    assert_eq!(read_user_bets(&market, &alice.pubkey()).len(), MAX_BETS_PER_USER);

    // With the index full, any new position is rejected - on this event or another.
    let one_too_many = send_transaction_from_instructions(
        &mut market.svm,
        vec![place_bet_ix(
            mint,
            &alice.pubkey(),
            &alice_ata,
            full_event_id,
            MAX_BETS_PER_USER as u8,
            STAKE,
        )],
        &[&alice],
        &alice.pubkey(),
    );
    assert!(one_too_many.is_err(), "a full index must reject a new position");
    let other_market_bet = send_transaction_from_instructions(
        &mut market.svm,
        vec![place_bet_ix(mint, &alice.pubkey(), &alice_ata, second_event_id, 0, STAKE)],
        &[&alice],
        &alice.pubkey(),
    );
    assert!(other_market_bet.is_err(), "a full index must reject bets on any market");

    // Unwind one position: cancel the event and refund the first bet.
    send_transaction_from_instructions(
        &mut market.svm,
        vec![cancel_event_ix(admin, full_event_id)],
        &[&market.admin],
        &admin,
    )
    .unwrap();
    send_transaction_from_instructions(
        &mut market.svm,
        vec![claim_refund_ix(mint, &alice.pubkey(), &alice_ata, full_event_id, 0)],
        &[&alice],
        &alice.pubkey(),
    )
    .unwrap();
    let bets_after_refund = read_user_bets(&market, &alice.pubkey());
    assert_eq!(bets_after_refund.len(), MAX_BETS_PER_USER - 1);
    let refunded_bet = bet_pda(&outcome_pda(&event_pda(full_event_id), 0), &alice.pubkey());
    assert!(
        !bets_after_refund.contains(&refunded_bet),
        "the refunded bet must leave the index"
    );

    // The freed slot lets the wallet bet again. A fresh blockhash so this is
    // a distinct transaction from the rejected attempt above.
    market.svm.expire_blockhash();
    send_transaction_from_instructions(
        &mut market.svm,
        vec![place_bet_ix(mint, &alice.pubkey(), &alice_ata, second_event_id, 0, STAKE)],
        &[&alice],
        &alice.pubkey(),
    )
    .unwrap();
    let final_bets = read_user_bets(&market, &alice.pubkey());
    assert_eq!(final_bets.len(), MAX_BETS_PER_USER);
    let new_bet = bet_pda(&outcome_pda(&event_pda(second_event_id), 0), &alice.pubkey());
    assert!(final_bets.contains(&new_bet), "the new position must appear in the index");
}

// The outcome list is final once betting opens, and nobody can bet before it
// is. A bet on a draft would otherwise let anyone freeze a half-built market,
// and an outcome added after a bet would change the question under it.
#[test]
fn test_outcomes_lock_when_betting_opens() {
    let mut market = setup();
    let event_id: u64 = 9;
    let (alice, alice_ata) = create_bettor(&mut market, 1_000);

    init_config(&mut market);
    let admin = market.admin.pubkey();
    let mint = market.mint;
    send_transaction_from_instructions(
        &mut market.svm,
        vec![
            initialize_event_ix(admin, mint, event_id, "Top-grossing film"),
            add_outcome_ix(admin, event_id, 0, "Toy Story 5"),
        ],
        &[&market.admin],
        &admin,
    )
    .unwrap();

    // The draft has one outcome and no bettor can touch it yet.
    let early_bet = send_transaction_from_instructions(
        &mut market.svm,
        vec![place_bet_ix(mint, &alice.pubkey(), &alice_ata, event_id, 0, 100)],
        &[&alice],
        &alice.pubkey(),
    );
    assert_fails_with(early_bet, BettingError::EventNotOpen);

    // The admin finishes the list and opens the market.
    send_transaction_from_instructions(
        &mut market.svm,
        vec![
            add_outcome_ix(admin, event_id, 1, "Backrooms"),
            open_betting_ix(admin, event_id),
        ],
        &[&market.admin],
        &admin,
    )
    .unwrap();

    // Now the list is fixed, with or without money in the pool.
    let late_outcome = send_transaction_from_instructions(
        &mut market.svm,
        vec![add_outcome_ix(admin, event_id, 2, "Late entry")],
        &[&market.admin],
        &admin,
    );
    assert_fails_with(late_outcome, BettingError::EventNotDraft);

    // Opening twice is refused too.
    let reopen = send_transaction_from_instructions(
        &mut market.svm,
        vec![open_betting_ix(admin, event_id)],
        &[&market.admin],
        &admin,
    );
    assert_fails_with(reopen, BettingError::EventNotDraft);
}

#[test]
fn test_open_betting_needs_two_outcomes() {
    let mut market = setup();
    let event_id: u64 = 10;

    init_config(&mut market);
    let admin = market.admin.pubkey();
    let mint = market.mint;
    send_transaction_from_instructions(
        &mut market.svm,
        vec![
            initialize_event_ix(admin, mint, event_id, "One horse race"),
            add_outcome_ix(admin, event_id, 0, "The only horse"),
        ],
        &[&market.admin],
        &admin,
    )
    .unwrap();

    let result = send_transaction_from_instructions(
        &mut market.svm,
        vec![open_betting_ix(admin, event_id)],
        &[&market.admin],
        &admin,
    );
    assert_fails_with(result, BettingError::NotEnoughOutcomes);

    // Only the admin can open a market.
    send_transaction_from_instructions(
        &mut market.svm,
        vec![add_outcome_ix(admin, event_id, 1, "A second horse")],
        &[&market.admin],
        &admin,
    )
    .unwrap();
    let mallory = create_wallet(&mut market.svm, 10_000_000_000).unwrap();
    let unauthorized = send_transaction_from_instructions(
        &mut market.svm,
        vec![open_betting_ix(mallory.pubkey(), event_id)],
        &[&mallory],
        &mallory.pubkey(),
    );
    assert_fails_with(unauthorized, BettingError::Unauthorized);
}

// Bets land strictly before the close time and settlement only at or after
// it, so there is no second at which someone who knows the result can still
// stake, and none at which the admin can end the market early.
#[test]
fn test_betting_closes_at_close_time() {
    let mut market = setup();
    let event_id: u64 = 11;
    let (alice, alice_ata) = create_bettor(&mut market, 1_000);
    let (carol, carol_ata) = create_bettor(&mut market, 1_000);

    init_config(&mut market);
    let admin = market.admin.pubkey();
    let mint = market.mint;
    let fee_recipient = market.fee_recipient.pubkey();
    let fee_recipient_ata = market.fee_recipient_ata;
    send_transaction_from_instructions(
        &mut market.svm,
        vec![
            initialize_event_ix(admin, mint, event_id, "Final score"),
            add_outcome_ix(admin, event_id, 0, "Home"),
            add_outcome_ix(admin, event_id, 1, "Away"),
            open_betting_ix(admin, event_id),
        ],
        &[&market.admin],
        &admin,
    )
    .unwrap();

    // One second before the close, a bet still lands...
    warp_to(&mut market.svm, BETTING_CLOSES_AT - 1);
    send_transaction_from_instructions(
        &mut market.svm,
        vec![place_bet_ix(mint, &alice.pubkey(), &alice_ata, event_id, 0, 100)],
        &[&alice],
        &alice.pubkey(),
    )
    .unwrap();

    // ...and the admin cannot settle yet.
    let early_settle = send_transaction_from_instructions(
        &mut market.svm,
        vec![settle_event_ix(admin, mint, fee_recipient, fee_recipient_ata, event_id, 0)],
        &[&market.admin],
        &admin,
    );
    assert_fails_with(early_settle, BettingError::BettingStillOpen);

    // At the close time, betting stops, even though the event is still Open.
    warp_to(&mut market.svm, BETTING_CLOSES_AT);
    let late_bet = send_transaction_from_instructions(
        &mut market.svm,
        vec![place_bet_ix(mint, &carol.pubkey(), &carol_ata, event_id, 1, 100)],
        &[&carol],
        &carol.pubkey(),
    );
    assert_fails_with(late_bet, BettingError::BettingClosed);

    send_transaction_from_instructions(
        &mut market.svm,
        vec![settle_event_ix(admin, mint, fee_recipient, fee_recipient_ata, event_id, 0)],
        &[&market.admin],
        &admin,
    )
    .unwrap();
}

#[test]
fn test_close_time_must_be_in_the_future() {
    let mut market = setup();
    init_config(&mut market);
    let admin = market.admin.pubkey();
    let mint = market.mint;

    let result = send_transaction_from_instructions(
        &mut market.svm,
        vec![initialize_event_closing_at_ix(admin, mint, 12, "Already over", START_TIME)],
        &[&market.admin],
        &admin,
    );
    assert_fails_with(result, BettingError::CloseTimeInPast);
}
