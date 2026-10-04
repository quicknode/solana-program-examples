use {
    anchor_lang::{
        prelude::Clock,
        solana_program::{
            instruction::Instruction, pubkey::Pubkey, system_instruction, system_program,
        },
        AccountDeserialize, InstructionData, ToAccountMetas,
    },
    betting_market::{error::BettingError, Bet, Event},
    litesvm::LiteSVM,
    solana_keypair::Keypair,
    solana_kite::{
        create_associated_token_account, create_token_mint, create_wallet, get_sol_balance,
        get_token_account_balance, mint_tokens_to_token_account,
        send_transaction_from_instructions, SolanaKiteError,
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
    Pubkey::find_program_address(
        &[b"outcome", event.as_ref(), &[index]],
        &betting_market::id(),
    )
    .0
}

fn bet_pda(outcome: &Pubkey, bettor: &Pubkey) -> Pubkey {
    Pubkey::find_program_address(
        &[b"bet", outcome.as_ref(), bettor.as_ref()],
        &betting_market::id(),
    )
    .0
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
    svm.add_program(betting_market::id(), program_bytes)
        .unwrap();

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

fn initialize_event_ix(
    admin: Pubkey,
    mint: Pubkey,
    event_id: u64,
    description: &str,
) -> Instruction {
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
        }
        .to_account_metas(None),
    )
}

fn close_outcome_ix(admin: Pubkey, event_id: u64, outcome_index: u8) -> Instruction {
    let event = event_pda(event_id);
    Instruction::new_with_bytes(
        betting_market::id(),
        &betting_market::instruction::CloseOutcome {}.data(),
        betting_market::accounts::CloseOutcomeAccountConstraints {
            admin,
            config: config_pda(),
            event,
            outcome: outcome_pda(&event, outcome_index),
        }
        .to_account_metas(None),
    )
}

fn close_event_ix(
    admin: Pubkey,
    mint: Pubkey,
    fee_recipient: Pubkey,
    fee_recipient_ata: Pubkey,
    event_id: u64,
) -> Instruction {
    let event = event_pda(event_id);
    Instruction::new_with_bytes(
        betting_market::id(),
        &betting_market::instruction::CloseEvent {}.data(),
        betting_market::accounts::CloseEventAccountConstraints {
            admin,
            config: config_pda(),
            token_mint: mint,
            event,
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

// A closed account has no lamports and no data. A Bet account lives only
// while its position is open: claiming, refunding or closing it as a loser
// closes the account. Outcome accounts, the vault and the Event account close
// through `close_outcome` and `close_event` once the event is finished.
fn account_is_open(market: &Market, address: &Pubkey) -> bool {
    market
        .svm
        .get_account(address)
        .is_some_and(|account| account.lamports > 0 && !account.data.is_empty())
}

// The lamports an open account holds, which closing it returns to whoever
// paid its rent.
fn rent_of(market: &Market, address: &Pubkey) -> u64 {
    market.svm.get_account(address).unwrap().lamports
}

// What one transaction with a single signer costs its fee payer, measured by
// sending the admin a zero-lamport transfer to themselves: nothing else in
// that transaction moves lamports. A fresh blockhash first, so repeated
// measurements are distinct transactions.
fn transaction_fee(market: &mut Market) -> u64 {
    let admin = market.admin.pubkey();
    market.svm.expire_blockhash();
    let before = get_sol_balance(&market.svm, &admin);
    send_transaction_from_instructions(
        &mut market.svm,
        vec![system_instruction::transfer(&admin, &admin, 0)],
        &[&market.admin],
        &admin,
    )
    .unwrap();
    before - get_sol_balance(&market.svm, &admin)
}

fn read_event(market: &Market, event_id: u64) -> Event {
    let account = market.svm.get_account(&event_pda(event_id)).unwrap();
    Event::try_deserialize(&mut account.data.as_slice()).unwrap()
}

// Closes a finished event's Outcome accounts and then the event itself, as
// the admin, asserting that each rent comes back to the admin and that every
// closed account is gone. Returns what the vault held before it closed.
fn close_outcomes_and_event(market: &mut Market, event_id: u64, outcome_count: u8) -> u64 {
    let admin = market.admin.pubkey();
    let mint = market.mint;
    let fee_recipient = market.fee_recipient.pubkey();
    let fee_recipient_ata = market.fee_recipient_ata;
    let event = event_pda(event_id);
    let vault = derive_ata(&event, &mint);
    let fee = transaction_fee(market);

    for index in 0..outcome_count {
        let outcome = outcome_pda(&event, index);
        let outcome_rent = rent_of(market, &outcome);
        let admin_before = get_sol_balance(&market.svm, &admin);
        send_transaction_from_instructions(
            &mut market.svm,
            vec![close_outcome_ix(admin, event_id, index)],
            &[&market.admin],
            &admin,
        )
        .unwrap();
        assert!(
            !account_is_open(market, &outcome),
            "outcome {index} must be closed"
        );
        assert_eq!(
            get_sol_balance(&market.svm, &admin),
            admin_before + outcome_rent - fee,
            "outcome {index}'s rent must return to the admin"
        );
    }
    assert_eq!(read_event(market, event_id).open_outcomes, 0);

    let vault_balance = get_token_account_balance(&market.svm, &vault).unwrap();
    let event_rent = rent_of(market, &event);
    let vault_rent = rent_of(market, &vault);
    let admin_before = get_sol_balance(&market.svm, &admin);
    send_transaction_from_instructions(
        &mut market.svm,
        vec![close_event_ix(
            admin,
            mint,
            fee_recipient,
            fee_recipient_ata,
            event_id,
        )],
        &[&market.admin],
        &admin,
    )
    .unwrap();
    assert!(!account_is_open(market, &event));
    assert!(!account_is_open(market, &vault));
    assert_eq!(
        get_sol_balance(&market.svm, &admin),
        admin_before + event_rent + vault_rent - fee,
        "the event's and the vault's rent must return to the admin"
    );
    vault_balance
}

// A bettor's open positions are the Bet accounts whose `bettor` field is their
// address. An RPC client finds them with getProgramAccounts and a memcmp filter
// at this offset, so the program keeps no per-wallet index (and no cap on
// how many positions a wallet holds).
const BET_BETTOR_OFFSET: usize = 8;

fn read_bet(market: &Market, bet: &Pubkey) -> Bet {
    let account = market.svm.get_account(bet).unwrap();
    Bet::try_deserialize(&mut account.data.as_slice()).unwrap()
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
        vec![place_bet_ix(
            mint,
            &alice.pubkey(),
            &alice_ata,
            event_id,
            0,
            100,
        )],
        &[&alice],
        &alice.pubkey(),
    )
    .unwrap();
    send_transaction_from_instructions(
        &mut market.svm,
        vec![place_bet_ix(
            mint,
            &bob.pubkey(),
            &bob_ata,
            event_id,
            0,
            300,
        )],
        &[&bob],
        &bob.pubkey(),
    )
    .unwrap();
    send_transaction_from_instructions(
        &mut market.svm,
        vec![place_bet_ix(
            mint,
            &carol.pubkey(),
            &carol_ata,
            event_id,
            1,
            200,
        )],
        &[&carol],
        &carol.pubkey(),
    )
    .unwrap();

    // Vault holds the entire pool.
    let vault = derive_ata(&event_pda(event_id), &mint);
    assert_eq!(get_token_account_balance(&market.svm, &vault).unwrap(), 600);
    // Alice's position is a Bet account that records her as the bettor, at
    // the offset a client filters on to list her open bets.
    let alice_bet_address = bet_pda(&outcome_pda(&event_pda(event_id), 0), &alice.pubkey());
    assert_eq!(read_bet(&market, &alice_bet_address).bettor, alice.pubkey());
    let alice_bet_data = market.svm.get_account(&alice_bet_address).unwrap().data;
    assert_eq!(
        &alice_bet_data[BET_BETTOR_OFFSET..BET_BETTOR_OFFSET + 32],
        alice.pubkey().as_ref()
    );

    // Settle to "Yes" (index 0). Losing pool 200, fee = 2% = 4, distributable = 196.
    let fee_recipient = market.fee_recipient.pubkey();
    let fee_recipient_ata = market.fee_recipient_ata;
    // Betting has closed, so the event can be settled.
    warp_to(&mut market.svm, BETTING_CLOSES_AT);
    send_transaction_from_instructions(
        &mut market.svm,
        vec![settle_event_ix(
            admin,
            mint,
            fee_recipient,
            fee_recipient_ata,
            event_id,
            0,
        )],
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
        vec![claim_winnings_ix(
            mint,
            &alice.pubkey(),
            &alice_ata,
            event_id,
            0,
        )],
        &[&alice],
        &alice.pubkey(),
    )
    .unwrap();
    send_transaction_from_instructions(
        &mut market.svm,
        vec![claim_winnings_ix(
            mint,
            &bob.pubkey(),
            &bob_ata,
            event_id,
            0,
        )],
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

    // Claiming closed the winners' Bet accounts.
    let winning_outcome = outcome_pda(&event_pda(event_id), 0);
    assert!(!account_is_open(
        &market,
        &bet_pda(&winning_outcome, &alice.pubkey())
    ));
    assert!(!account_is_open(
        &market,
        &bet_pda(&winning_outcome, &bob.pubkey())
    ));

    // Carol bet the losing outcome, so she has nothing to claim.
    let carol_claim = send_transaction_from_instructions(
        &mut market.svm,
        vec![claim_winnings_ix(
            mint,
            &carol.pubkey(),
            &carol_ata,
            event_id,
            1,
        )],
        &[&carol],
        &carol.pubkey(),
    );
    assert_fails_with(carol_claim, BettingError::NothingToClaim);

    // Her losing position stays open until she closes it.
    let carol_bet = bet_pda(&outcome_pda(&event_pda(event_id), 1), &carol.pubkey());
    assert!(account_is_open(&market, &carol_bet));
    send_transaction_from_instructions(
        &mut market.svm,
        vec![close_losing_bet_ix(&carol.pubkey(), event_id, 1)],
        &[&carol],
        &carol.pubkey(),
    )
    .unwrap();
    assert!(!account_is_open(&market, &carol_bet));
}

#[test]
fn test_only_admin_can_initialize_event() {
    let mut market = setup();
    init_config(&mut market);

    let admin = market.admin.pubkey();
    let mint = market.mint;
    let event_id: u64 = 7;
    let mallory = create_wallet(&mut market.svm, 10_000_000_000).unwrap();
    let result = send_transaction_from_instructions(
        &mut market.svm,
        vec![initialize_event_ix(
            mallory.pubkey(),
            mint,
            event_id,
            "Unauthorized event",
        )],
        &[&mallory],
        &mallory.pubkey(),
    );
    assert_fails_with(result, BettingError::Unauthorized);

    // Nor can anyone but the admin add an outcome to a draft, or open it.
    send_transaction_from_instructions(
        &mut market.svm,
        vec![
            initialize_event_ix(admin, mint, event_id, "Admin's event"),
            add_outcome_ix(admin, event_id, 0, "Glowbugs 3"),
            add_outcome_ix(admin, event_id, 1, "The Quiet Floor"),
        ],
        &[&market.admin],
        &admin,
    )
    .unwrap();
    let unauthorized_outcome = send_transaction_from_instructions(
        &mut market.svm,
        vec![add_outcome_ix(mallory.pubkey(), event_id, 2, "Late entry")],
        &[&mallory],
        &mallory.pubkey(),
    );
    assert_fails_with(unauthorized_outcome, BettingError::Unauthorized);
    let unauthorized_open = send_transaction_from_instructions(
        &mut market.svm,
        vec![open_betting_ix(mallory.pubkey(), event_id)],
        &[&mallory],
        &mallory.pubkey(),
    );
    assert_fails_with(unauthorized_open, BettingError::Unauthorized);
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
        vec![place_bet_ix(
            mint,
            &alice.pubkey(),
            &alice_ata,
            event_id,
            0,
            100,
        )],
        &[&alice],
        &alice.pubkey(),
    )
    .unwrap();
    // Betting has closed, so the event can be settled.
    warp_to(&mut market.svm, BETTING_CLOSES_AT);
    send_transaction_from_instructions(
        &mut market.svm,
        vec![settle_event_ix(
            admin,
            mint,
            fee_recipient,
            fee_recipient_ata,
            event_id,
            0,
        )],
        &[&market.admin],
        &admin,
    )
    .unwrap();

    let late_bet = send_transaction_from_instructions(
        &mut market.svm,
        vec![place_bet_ix(
            mint,
            &bob.pubkey(),
            &bob_ata,
            event_id,
            1,
            100,
        )],
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
        vec![place_bet_ix(
            mint,
            &alice.pubkey(),
            &alice_ata,
            event_id,
            0,
            100,
        )],
        &[&alice],
        &alice.pubkey(),
    )
    .unwrap();
    send_transaction_from_instructions(
        &mut market.svm,
        vec![place_bet_ix(
            mint,
            &carol.pubkey(),
            &carol_ata,
            event_id,
            1,
            100,
        )],
        &[&carol],
        &carol.pubkey(),
    )
    .unwrap();
    // Betting has closed, so the event can be settled.
    warp_to(&mut market.svm, BETTING_CLOSES_AT);
    send_transaction_from_instructions(
        &mut market.svm,
        vec![settle_event_ix(
            admin,
            mint,
            fee_recipient,
            fee_recipient_ata,
            event_id,
            0,
        )],
        &[&market.admin],
        &admin,
    )
    .unwrap();

    send_transaction_from_instructions(
        &mut market.svm,
        vec![claim_winnings_ix(
            mint,
            &alice.pubkey(),
            &alice_ata,
            event_id,
            0,
        )],
        &[&alice],
        &alice.pubkey(),
    )
    .unwrap();
    // A fresh blockhash so the second claim is a distinct transaction.
    market.svm.expire_blockhash();
    let second = send_transaction_from_instructions(
        &mut market.svm,
        vec![claim_winnings_ix(
            mint,
            &alice.pubkey(),
            &alice_ata,
            event_id,
            0,
        )],
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
        vec![place_bet_ix(
            mint,
            &alice.pubkey(),
            &alice_ata,
            event_id,
            0,
            100,
        )],
        &[&alice],
        &alice.pubkey(),
    )
    .unwrap();

    // Betting has closed, so the event can be settled.
    warp_to(&mut market.svm, BETTING_CLOSES_AT);
    let result = send_transaction_from_instructions(
        &mut market.svm,
        vec![settle_event_ix(
            admin,
            mint,
            fee_recipient,
            fee_recipient_ata,
            event_id,
            1,
        )],
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
        vec![place_bet_ix(
            mint,
            &alice.pubkey(),
            &alice_ata,
            event_id,
            0,
            250,
        )],
        &[&alice],
        &alice.pubkey(),
    )
    .unwrap();
    send_transaction_from_instructions(
        &mut market.svm,
        vec![place_bet_ix(
            mint,
            &carol.pubkey(),
            &carol_ata,
            event_id,
            1,
            750,
        )],
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
    assert!(account_is_open(&market, &alice_bet));

    send_transaction_from_instructions(
        &mut market.svm,
        vec![claim_refund_ix(
            mint,
            &alice.pubkey(),
            &alice_ata,
            event_id,
            0,
        )],
        &[&alice],
        &alice.pubkey(),
    )
    .unwrap();

    // The refund closed Alice's Bet account.
    assert!(!account_is_open(&market, &alice_bet));

    // Carol's refund is still outstanding, so the event's accounts stay.
    let outcome_still_in_use = send_transaction_from_instructions(
        &mut market.svm,
        vec![close_outcome_ix(admin, event_id, 0)],
        &[&market.admin],
        &admin,
    );
    assert_fails_with(outcome_still_in_use, BettingError::BetsStillOpen);
    assert_eq!(read_event(&market, event_id).open_bets, 1);

    send_transaction_from_instructions(
        &mut market.svm,
        vec![claim_refund_ix(
            mint,
            &carol.pubkey(),
            &carol_ata,
            event_id,
            1,
        )],
        &[&carol],
        &carol.pubkey(),
    )
    .unwrap();

    // Both bettors made whole; no fee on a cancelled event.
    assert_eq!(
        get_token_account_balance(&market.svm, &alice_ata).unwrap(),
        1_000
    );
    assert_eq!(
        get_token_account_balance(&market.svm, &carol_ata).unwrap(),
        1_000
    );
    let vault = derive_ata(&event_pda(event_id), &mint);
    assert_eq!(get_token_account_balance(&market.svm, &vault).unwrap(), 0);

    // With every stake refunded the admin closes the outcomes and the event.
    // The vault was empty, so the fee recipient receives nothing.
    assert_eq!(read_event(&market, event_id).open_bets, 0);
    let vault_balance_at_close = close_outcomes_and_event(&mut market, event_id, 2);
    assert_eq!(vault_balance_at_close, 0);
    assert_eq!(
        get_token_account_balance(&market.svm, &market.fee_recipient_ata).unwrap(),
        0
    );
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
        vec![place_bet_ix(
            mint,
            &alice.pubkey(),
            &alice_ata,
            event_id,
            0,
            100,
        )],
        &[&alice],
        &alice.pubkey(),
    )
    .unwrap();
    send_transaction_from_instructions(
        &mut market.svm,
        vec![place_bet_ix(
            mint,
            &carol.pubkey(),
            &carol_ata,
            event_id,
            1,
            100,
        )],
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
    assert_fails_with(premature_close, BettingError::EventNotSettled);

    // Betting has closed, so the event can be settled.
    warp_to(&mut market.svm, BETTING_CLOSES_AT);
    send_transaction_from_instructions(
        &mut market.svm,
        vec![settle_event_ix(
            admin,
            mint,
            fee_recipient,
            fee_recipient_ata,
            event_id,
            0,
        )],
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
    assert_fails_with(winner_close, BettingError::BetWon);
    let alice_bet = bet_pda(&outcome_pda(&event_pda(event_id), 0), &alice.pubkey());
    assert!(account_is_open(&market, &alice_bet));

    // Carol lost; closing returns her Bet account's rent. A fresh blockhash so this is
    // a distinct transaction from her premature attempt above.
    market.svm.expire_blockhash();
    send_transaction_from_instructions(
        &mut market.svm,
        vec![close_losing_bet_ix(&carol.pubkey(), event_id, 1)],
        &[&carol],
        &carol.pubkey(),
    )
    .unwrap();
    let carol_bet = bet_pda(&outcome_pda(&event_pda(event_id), 1), &carol.pubkey());
    assert!(!account_is_open(&market, &carol_bet));
}

// A wallet can hold as many open positions as it likes: forty bets across
// forty outcomes, plus one on another market, all land. The program keeps no
// per-wallet index of open bets, so there is no list to fill.
#[test]
fn test_no_cap_on_open_bets_per_wallet() {
    const STAKE: u64 = 10;
    const OUTCOME_COUNT: u8 = 40;
    let mut market = setup();
    let wide_event_id: u64 = 7;
    let second_event_id: u64 = 8;
    let (alice, alice_ata) = create_bettor(&mut market, (OUTCOME_COUNT as u64 + 1) * STAKE);

    init_config(&mut market);
    let admin = market.admin.pubkey();
    let mint = market.mint;

    send_transaction_from_instructions(
        &mut market.svm,
        vec![initialize_event_ix(
            admin,
            mint,
            wide_event_id,
            "Wide field",
        )],
        &[&market.admin],
        &admin,
    )
    .unwrap();
    for index in 0..OUTCOME_COUNT {
        send_transaction_from_instructions(
            &mut market.svm,
            vec![add_outcome_ix(
                admin,
                wide_event_id,
                index,
                &format!("Runner {index}"),
            )],
            &[&market.admin],
            &admin,
        )
        .unwrap();
    }
    send_transaction_from_instructions(
        &mut market.svm,
        vec![open_betting_ix(admin, wide_event_id)],
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

    for index in 0..OUTCOME_COUNT {
        send_transaction_from_instructions(
            &mut market.svm,
            vec![place_bet_ix(
                mint,
                &alice.pubkey(),
                &alice_ata,
                wide_event_id,
                index,
                STAKE,
            )],
            &[&alice],
            &alice.pubkey(),
        )
        .unwrap();
    }
    send_transaction_from_instructions(
        &mut market.svm,
        vec![place_bet_ix(
            mint,
            &alice.pubkey(),
            &alice_ata,
            second_event_id,
            0,
            STAKE,
        )],
        &[&alice],
        &alice.pubkey(),
    )
    .unwrap();

    for index in 0..OUTCOME_COUNT {
        let bet = bet_pda(
            &outcome_pda(&event_pda(wide_event_id), index),
            &alice.pubkey(),
        );
        assert!(account_is_open(&market, &bet), "bet {index} must be open");
    }
    let other_market_bet = bet_pda(
        &outcome_pda(&event_pda(second_event_id), 0),
        &alice.pubkey(),
    );
    assert!(account_is_open(&market, &other_market_bet));
    assert_eq!(
        get_token_account_balance(&market.svm, &alice_ata).unwrap(),
        0
    );
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
            add_outcome_ix(admin, event_id, 0, "Glowbugs 3"),
        ],
        &[&market.admin],
        &admin,
    )
    .unwrap();

    // The draft has one outcome and no bettor can touch it yet.
    let early_bet = send_transaction_from_instructions(
        &mut market.svm,
        vec![place_bet_ix(
            mint,
            &alice.pubkey(),
            &alice_ata,
            event_id,
            0,
            100,
        )],
        &[&alice],
        &alice.pubkey(),
    );
    assert_fails_with(early_bet, BettingError::EventNotOpen);

    // The admin finishes the list and opens the market.
    send_transaction_from_instructions(
        &mut market.svm,
        vec![
            add_outcome_ix(admin, event_id, 1, "The Quiet Floor"),
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
        vec![place_bet_ix(
            mint,
            &alice.pubkey(),
            &alice_ata,
            event_id,
            0,
            100,
        )],
        &[&alice],
        &alice.pubkey(),
    )
    .unwrap();

    // ...and the admin cannot settle yet.
    let early_settle = send_transaction_from_instructions(
        &mut market.svm,
        vec![settle_event_ix(
            admin,
            mint,
            fee_recipient,
            fee_recipient_ata,
            event_id,
            0,
        )],
        &[&market.admin],
        &admin,
    );
    assert_fails_with(early_settle, BettingError::BettingStillOpen);

    // At the close time, betting stops, even though the event is still Open.
    warp_to(&mut market.svm, BETTING_CLOSES_AT);
    let late_bet = send_transaction_from_instructions(
        &mut market.svm,
        vec![place_bet_ix(
            mint,
            &carol.pubkey(),
            &carol_ata,
            event_id,
            1,
            100,
        )],
        &[&carol],
        &carol.pubkey(),
    );
    assert_fails_with(late_bet, BettingError::BettingClosed);

    send_transaction_from_instructions(
        &mut market.svm,
        vec![settle_event_ix(
            admin,
            mint,
            fee_recipient,
            fee_recipient_ata,
            event_id,
            0,
        )],
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
        vec![initialize_event_closing_at_ix(
            admin,
            mint,
            12,
            "Already over",
            START_TIME,
        )],
        &[&market.admin],
        &admin,
    );
    assert_fails_with(result, BettingError::CloseTimeInPast);
}

// Sets up a two-outcome market ("Yes" at index 0, "No" at index 1) on
// `event_id` and opens it to bets.
fn open_yes_no_market(market: &mut Market, event_id: u64, description: &str) {
    let admin = market.admin.pubkey();
    let mint = market.mint;
    send_transaction_from_instructions(
        &mut market.svm,
        vec![
            initialize_event_ix(admin, mint, event_id, description),
            add_outcome_ix(admin, event_id, 0, "Yes"),
            add_outcome_ix(admin, event_id, 1, "No"),
            open_betting_ix(admin, event_id),
        ],
        &[&market.admin],
        &admin,
    )
    .unwrap();
}

fn place_bet(
    market: &mut Market,
    bettor: &Keypair,
    bettor_ata: &Pubkey,
    event_id: u64,
    outcome_index: u8,
    amount: u64,
) {
    let mint = market.mint;
    send_transaction_from_instructions(
        &mut market.svm,
        vec![place_bet_ix(
            mint,
            &bettor.pubkey(),
            bettor_ata,
            event_id,
            outcome_index,
            amount,
        )],
        &[bettor],
        &bettor.pubkey(),
    )
    .unwrap();
}

// Settles `event_id` to `winning_outcome_index` once betting has closed.
fn settle(market: &mut Market, event_id: u64, winning_outcome_index: u8) {
    let admin = market.admin.pubkey();
    let mint = market.mint;
    let fee_recipient = market.fee_recipient.pubkey();
    let fee_recipient_ata = market.fee_recipient_ata;
    warp_to(&mut market.svm, BETTING_CLOSES_AT);
    send_transaction_from_instructions(
        &mut market.svm,
        vec![settle_event_ix(
            admin,
            mint,
            fee_recipient,
            fee_recipient_ata,
            event_id,
            winning_outcome_index,
        )],
        &[&market.admin],
        &admin,
    )
    .unwrap();
}

// The whole lifecycle through to every account closing. Stakes are chosen so
// the pro-rata split does not divide evenly: Yes pool 300 (Alice 100 in two
// bets, Bob 200), No pool 250 (Carol). Yes wins: losing pool 250, fee 5,
// distributable 245; Alice gets floor(100 * 245 / 300) = 81 and Bob
// floor(200 * 245 / 300) = 163, so one minor unit of dust stays in the vault
// until `close_event` pays it to the fee recipient.
#[test]
fn test_close_event_pays_dust_to_fee_recipient_and_returns_rent() {
    let mut market = setup();
    let event_id: u64 = 13;
    let (alice, alice_ata) = create_bettor(&mut market, 1_000);
    let (bob, bob_ata) = create_bettor(&mut market, 1_000);
    let (carol, carol_ata) = create_bettor(&mut market, 1_000);

    init_config(&mut market);
    let admin = market.admin.pubkey();
    let mint = market.mint;
    let fee_recipient = market.fee_recipient.pubkey();
    let fee_recipient_ata = market.fee_recipient_ata;
    open_yes_no_market(&mut market, event_id, "Top-grossing film");

    // Alice's second bet tops up her existing Bet account rather than
    // creating another, so it counts once among the open bets.
    place_bet(&mut market, &alice, &alice_ata, event_id, 0, 60);
    place_bet(&mut market, &alice, &alice_ata, event_id, 0, 40);
    place_bet(&mut market, &bob, &bob_ata, event_id, 0, 200);
    place_bet(&mut market, &carol, &carol_ata, event_id, 1, 250);
    let event = read_event(&market, event_id);
    assert_eq!(event.open_bets, 3);
    assert_eq!(event.open_outcomes, 2);
    assert_eq!(event.total_pool, 550);

    settle(&mut market, event_id, 0);
    assert_eq!(
        get_token_account_balance(&market.svm, &fee_recipient_ata).unwrap(),
        5
    );

    // Carol closes her losing bet; Alice and Bob claim.
    send_transaction_from_instructions(
        &mut market.svm,
        vec![close_losing_bet_ix(&carol.pubkey(), event_id, 1)],
        &[&carol],
        &carol.pubkey(),
    )
    .unwrap();
    assert_eq!(read_event(&market, event_id).open_bets, 2);
    for (winner, winner_ata) in [(&alice, &alice_ata), (&bob, &bob_ata)] {
        send_transaction_from_instructions(
            &mut market.svm,
            vec![claim_winnings_ix(
                mint,
                &winner.pubkey(),
                winner_ata,
                event_id,
                0,
            )],
            &[winner],
            &winner.pubkey(),
        )
        .unwrap();
    }
    assert_eq!(
        get_token_account_balance(&market.svm, &alice_ata).unwrap(),
        1_000 - 100 + 181
    );
    assert_eq!(
        get_token_account_balance(&market.svm, &bob_ata).unwrap(),
        1_000 - 200 + 363
    );
    assert_eq!(read_event(&market, event_id).open_bets, 0);

    // The two floors left one minor unit in the vault.
    let vault = derive_ata(&event_pda(event_id), &mint);
    assert_eq!(get_token_account_balance(&market.svm, &vault).unwrap(), 1);

    // Outcome accounts close before the event does.
    let outcomes_still_open = send_transaction_from_instructions(
        &mut market.svm,
        vec![close_event_ix(
            admin,
            mint,
            fee_recipient,
            fee_recipient_ata,
            event_id,
        )],
        &[&market.admin],
        &admin,
    );
    assert_fails_with(outcomes_still_open, BettingError::OutcomesStillOpen);

    let vault_balance_at_close = close_outcomes_and_event(&mut market, event_id, 2);
    assert_eq!(vault_balance_at_close, 1);
    // The dust joined the fee: 5 + 1.
    assert_eq!(
        get_token_account_balance(&market.svm, &fee_recipient_ata).unwrap(),
        6
    );
}

// A settled event keeps its accounts while any Bet account of it is open:
// the claim and the losing-bet close both read the event and the outcome.
#[test]
fn test_close_event_refused_while_a_bet_is_open() {
    let mut market = setup();
    let event_id: u64 = 14;
    let (alice, alice_ata) = create_bettor(&mut market, 1_000);
    let (carol, carol_ata) = create_bettor(&mut market, 1_000);

    init_config(&mut market);
    let admin = market.admin.pubkey();
    let mint = market.mint;
    let fee_recipient = market.fee_recipient.pubkey();
    let fee_recipient_ata = market.fee_recipient_ata;
    open_yes_no_market(&mut market, event_id, "Match winner");
    place_bet(&mut market, &alice, &alice_ata, event_id, 0, 100);
    place_bet(&mut market, &carol, &carol_ata, event_id, 1, 300);
    settle(&mut market, event_id, 0);

    // Alice has claimed; Carol's losing bet is still open.
    send_transaction_from_instructions(
        &mut market.svm,
        vec![claim_winnings_ix(
            mint,
            &alice.pubkey(),
            &alice_ata,
            event_id,
            0,
        )],
        &[&alice],
        &alice.pubkey(),
    )
    .unwrap();
    assert_eq!(read_event(&market, event_id).open_bets, 1);

    let event_close = send_transaction_from_instructions(
        &mut market.svm,
        vec![close_event_ix(
            admin,
            mint,
            fee_recipient,
            fee_recipient_ata,
            event_id,
        )],
        &[&market.admin],
        &admin,
    );
    assert_fails_with(event_close, BettingError::BetsStillOpen);
    let outcome_close = send_transaction_from_instructions(
        &mut market.svm,
        vec![close_outcome_ix(admin, event_id, 1)],
        &[&market.admin],
        &admin,
    );
    assert_fails_with(outcome_close, BettingError::BetsStillOpen);

    // Once Carol closes her bet, everything else can close.
    send_transaction_from_instructions(
        &mut market.svm,
        vec![close_losing_bet_ix(&carol.pubkey(), event_id, 1)],
        &[&carol],
        &carol.pubkey(),
    )
    .unwrap();
    close_outcomes_and_event(&mut market, event_id, 2);
}

// A draft or open event has not finished, so its accounts cannot close,
// even when nobody has bet. Cancelling it is what lets it close.
#[test]
fn test_close_event_refused_while_event_is_open() {
    let mut market = setup();
    let event_id: u64 = 15;

    init_config(&mut market);
    let admin = market.admin.pubkey();
    let mint = market.mint;
    let fee_recipient = market.fee_recipient.pubkey();
    let fee_recipient_ata = market.fee_recipient_ata;
    send_transaction_from_instructions(
        &mut market.svm,
        vec![
            initialize_event_ix(admin, mint, event_id, "Never bet on"),
            add_outcome_ix(admin, event_id, 0, "Yes"),
            add_outcome_ix(admin, event_id, 1, "No"),
        ],
        &[&market.admin],
        &admin,
    )
    .unwrap();

    // As a draft...
    let draft_close = send_transaction_from_instructions(
        &mut market.svm,
        vec![close_event_ix(
            admin,
            mint,
            fee_recipient,
            fee_recipient_ata,
            event_id,
        )],
        &[&market.admin],
        &admin,
    );
    assert_fails_with(draft_close, BettingError::EventNotFinished);
    let draft_outcome_close = send_transaction_from_instructions(
        &mut market.svm,
        vec![close_outcome_ix(admin, event_id, 0)],
        &[&market.admin],
        &admin,
    );
    assert_fails_with(draft_outcome_close, BettingError::EventNotFinished);

    // ...and once open. A fresh blockhash so this is a distinct transaction
    // from the draft attempt above.
    send_transaction_from_instructions(
        &mut market.svm,
        vec![open_betting_ix(admin, event_id)],
        &[&market.admin],
        &admin,
    )
    .unwrap();
    market.svm.expire_blockhash();
    let open_close = send_transaction_from_instructions(
        &mut market.svm,
        vec![close_event_ix(
            admin,
            mint,
            fee_recipient,
            fee_recipient_ata,
            event_id,
        )],
        &[&market.admin],
        &admin,
    );
    assert_fails_with(open_close, BettingError::EventNotFinished);

    // Cancelled with no bets, it closes straight away.
    send_transaction_from_instructions(
        &mut market.svm,
        vec![cancel_event_ix(admin, event_id)],
        &[&market.admin],
        &admin,
    )
    .unwrap();
    let vault_balance_at_close = close_outcomes_and_event(&mut market, event_id, 2);
    assert_eq!(vault_balance_at_close, 0);
}

#[test]
fn test_only_admin_can_close_outcomes_and_event() {
    let mut market = setup();
    let event_id: u64 = 16;
    let (alice, alice_ata) = create_bettor(&mut market, 1_000);
    let (carol, carol_ata) = create_bettor(&mut market, 1_000);

    init_config(&mut market);
    let mint = market.mint;
    let fee_recipient = market.fee_recipient.pubkey();
    let fee_recipient_ata = market.fee_recipient_ata;
    open_yes_no_market(&mut market, event_id, "Derby winner");
    place_bet(&mut market, &alice, &alice_ata, event_id, 0, 100);
    place_bet(&mut market, &carol, &carol_ata, event_id, 1, 300);
    settle(&mut market, event_id, 0);
    send_transaction_from_instructions(
        &mut market.svm,
        vec![claim_winnings_ix(
            mint,
            &alice.pubkey(),
            &alice_ata,
            event_id,
            0,
        )],
        &[&alice],
        &alice.pubkey(),
    )
    .unwrap();
    send_transaction_from_instructions(
        &mut market.svm,
        vec![close_losing_bet_ix(&carol.pubkey(), event_id, 1)],
        &[&carol],
        &carol.pubkey(),
    )
    .unwrap();

    // Every bet is closed, so only the signer stands between Mallory and the
    // rent.
    let mallory = create_wallet(&mut market.svm, 10_000_000_000).unwrap();
    let outcome_close = send_transaction_from_instructions(
        &mut market.svm,
        vec![close_outcome_ix(mallory.pubkey(), event_id, 0)],
        &[&mallory],
        &mallory.pubkey(),
    );
    assert_fails_with(outcome_close, BettingError::Unauthorized);
    let event_close = send_transaction_from_instructions(
        &mut market.svm,
        vec![close_event_ix(
            mallory.pubkey(),
            mint,
            fee_recipient,
            fee_recipient_ata,
            event_id,
        )],
        &[&mallory],
        &mallory.pubkey(),
    );
    assert_fails_with(event_close, BettingError::Unauthorized);
    let event = event_pda(event_id);
    assert!(account_is_open(&market, &event));
    assert!(account_is_open(&market, &outcome_pda(&event, 0)));

    close_outcomes_and_event(&mut market, event_id, 2);
}
