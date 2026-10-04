//! quasar-test integration tests. They drive the real program instructions
//! end-to-end: initialize the config, create an event, add outcomes, open
//! betting, place bets, settle, and claim, asserting onchain state and token
//! balances at each step.

use {
    crate::{
        cpi::{
            AddOutcomeInstruction, CancelEventInstruction, ClaimRefundInstruction,
            ClaimWinningsInstruction, CloseEventInstruction, CloseLosingBetInstruction,
            CloseOutcomeInstruction, InitializeConfigInstruction, InitializeEventInstruction,
            OpenBettingInstruction, PlaceBetInstruction, SettleEventInstruction,
        },
        errors::BettingError,
        state::{Bet, Config, Event, EventStatus, EventVaultPda, Outcome},
    },
    quasar_test::prelude::*,
};

// Deterministic addresses keep tests independent of discovery order.
const ADMIN: Pubkey = Pubkey::new_from_array([1; 32]);
const FEE_RECIPIENT: Pubkey = Pubkey::new_from_array([2; 32]);
const TOKEN_MINT: Pubkey = Pubkey::new_from_array([3; 32]);
const FEE_RECIPIENT_TOKEN: Pubkey = Pubkey::new_from_array([4; 32]);
const BETTOR_A: Pubkey = Pubkey::new_from_array([5; 32]);
const BETTOR_B: Pubkey = Pubkey::new_from_array([6; 32]);
const TOKEN_A: Pubkey = Pubkey::new_from_array([7; 32]);
const TOKEN_B: Pubkey = Pubkey::new_from_array([8; 32]);
const ATTACKER: Pubkey = Pubkey::new_from_array([9; 32]);
const BETTOR_C: Pubkey = Pubkey::new_from_array([10; 32]);
const TOKEN_C: Pubkey = Pubkey::new_from_array([11; 32]);

const FEE_BPS: u16 = 100; // 1%
const DECIMALS: u8 = 6;
const STARTING_TOKENS: u64 = 1_000;
const EVENT_ID: u64 = 1;

// A fixed unix timestamp the clock is warped to before anything is written,
// so the betting window is deterministic: it closes a week later.
const START_TIME: i64 = 1_750_000_000;
const SECONDS_PER_DAY: i64 = 24 * 60 * 60;
const BETTING_CLOSES_AT: i64 = START_TIME + 7 * SECONDS_PER_DAY;

/// Set the clock to `START_TIME`, register the admin, the stake mint, and the
/// fee recipient's token account, then initialize the config.
fn base_world(test: &mut Test) {
    test.warp_to_timestamp(START_TIME);
    test.add(Wallet::new().at(ADMIN));
    test.add(Mint::new(ADMIN).at(TOKEN_MINT).decimals(DECIMALS));
    test.add(TokenAccount::new(TOKEN_MINT, FEE_RECIPIENT).at(FEE_RECIPIENT_TOKEN));
    test.send(InitializeConfigInstruction {
        admin: ADMIN,
        token_mint: TOKEN_MINT,
        default_fee_bps: FEE_BPS,
        fee_recipient: FEE_RECIPIENT,
    })
    .succeeds();
}

#[quasar_test]
fn initialize_config_records_admin_mint_and_fee(test: &mut Test) {
    base_world(test);

    let config = test.derive_pda(Config::seeds());
    let state = test.read::<Config>(config);
    assert_eq!(state.admin, ADMIN, "admin");
    assert_eq!(state.token_mint, TOKEN_MINT, "token_mint");
    assert_eq!(state.fee_recipient, FEE_RECIPIENT, "fee_recipient");
    assert_eq!(u16::from(state.default_fee_bps), FEE_BPS, "default_fee_bps");
}

/// Full parimutuel flow: two bettors stake on opposing outcomes, the admin
/// settles to the larger pool, the winner claims stake + share of the losing
/// pool (net of the 1% fee), and the loser closes their worthless bet.
///
/// A stakes 100 on outcome 0; B stakes 300 on outcome 1; outcome 1 wins.
/// losing_pool = 100, fee = 1, distributable = 99. B's winnings = 300*99/300 =
/// 99, payout = 399. Fee recipient gets 1. Vault ends empty.
#[quasar_test]
fn full_lifecycle_settles_and_pays_the_winner(test: &mut Test) {
    base_world(test);
    test.add(Wallet::new().at(BETTOR_A));
    test.add(Wallet::new().at(BETTOR_B));
    test.add(
        TokenAccount::new(TOKEN_MINT, BETTOR_A)
            .at(TOKEN_A)
            .amount(STARTING_TOKENS),
    );
    test.add(
        TokenAccount::new(TOKEN_MINT, BETTOR_B)
            .at(TOKEN_B)
            .amount(STARTING_TOKENS),
    );

    let event = test.derive_pda(Event::seeds(EVENT_ID));
    let vault = test.derive_pda(EventVaultPda::seeds(&event));
    let outcome0 = test.derive_pda(Outcome::seeds(&event, 0));
    let outcome1 = test.derive_pda(Outcome::seeds(&event, 1));
    let bet_a = test.derive_pda(Bet::seeds(&outcome0, &BETTOR_A));
    let bet_b = test.derive_pda(Bet::seeds(&outcome1, &BETTOR_B));

    const STAKE_A: u64 = 100;
    const STAKE_B: u64 = 300;
    const FEE: u64 = 1; // floor(100 * 100 / 10000) = 1
    const PAYOUT_B: u64 = STAKE_B + 99; // stake + winnings(99)

    test.send(InitializeEventInstruction {
        admin: ADMIN,
        token_mint: TOKEN_MINT,
        event_id: EVENT_ID,
        betting_closes_at: BETTING_CLOSES_AT,
        description: "Team A vs Team B".to_string().into(),
    })
    .succeeds();
    test.send(AddOutcomeInstruction {
        admin: ADMIN,
        event_event_id_seed: EVENT_ID,
        event_outcome_count_seed: 0,
        label: "Team A".to_string().into(),
    })
    .succeeds();
    test.send(AddOutcomeInstruction {
        admin: ADMIN,
        event_event_id_seed: EVENT_ID,
        event_outcome_count_seed: 1,
        label: "Team B".to_string().into(),
    })
    .succeeds();
    test.send(OpenBettingInstruction {
        admin: ADMIN,
        event_event_id_seed: EVENT_ID,
    })
    .succeeds();
    test.send(PlaceBetInstruction {
        bettor: BETTOR_A,
        token_mint: TOKEN_MINT,
        event_event_id_seed: EVENT_ID,
        outcome_index_seed: 0,
        bettor_token_account: TOKEN_A,
        amount: STAKE_A,
    })
    .succeeds();
    test.send(PlaceBetInstruction {
        bettor: BETTOR_B,
        token_mint: TOKEN_MINT,
        event_event_id_seed: EVENT_ID,
        outcome_index_seed: 1,
        bettor_token_account: TOKEN_B,
        amount: STAKE_B,
    })
    .succeeds();
    // Nothing closes before settlement.
    test.send(CloseLosingBetInstruction {
        bettor: BETTOR_A,
        event_event_id_seed: EVENT_ID,
        bet: bet_a,
    })
    .fails_with(BettingError::EventNotSettled);
    // Betting has closed, so the event can be settled.
    test.warp_to_timestamp(BETTING_CLOSES_AT);
    test.send(SettleEventInstruction {
        admin: ADMIN,
        token_mint: TOKEN_MINT,
        event_event_id_seed: EVENT_ID,
        fee_recipient_token_account: FEE_RECIPIENT_TOKEN,
        winning_outcome_index: 1,
    })
    .succeeds();
    // A winning bet is not closed as a loser, and a losing bet has nothing to
    // claim.
    test.send(CloseLosingBetInstruction {
        bettor: BETTOR_B,
        event_event_id_seed: EVENT_ID,
        bet: bet_b,
    })
    .fails_with(BettingError::BetWon);
    test.send(ClaimWinningsInstruction {
        bettor: BETTOR_A,
        token_mint: TOKEN_MINT,
        event_event_id_seed: EVENT_ID,
        bet: bet_a,
        bettor_token_account: TOKEN_A,
    })
    .fails_with(BettingError::NothingToClaim);
    test.send(ClaimWinningsInstruction {
        bettor: BETTOR_B,
        token_mint: TOKEN_MINT,
        event_event_id_seed: EVENT_ID,
        bet: bet_b,
        bettor_token_account: TOKEN_B,
    })
    .succeeds()
    .is_closed(bet_b);
    test.send(CloseLosingBetInstruction {
        bettor: BETTOR_A,
        event_event_id_seed: EVENT_ID,
        bet: bet_a,
    })
    .succeeds()
    .is_closed(bet_a);

    // Event settled with the recorded figures.
    let event_state = test.read::<Event>(event);
    assert_eq!(
        event_state.status,
        EventStatus::Settled as u8,
        "status settled"
    );
    assert_eq!(event_state.winning_outcome_index, 1, "winning index");
    assert_eq!(u64::from(event_state.winning_pool), STAKE_B, "winning pool");
    assert_eq!(
        u64::from(event_state.distributable_losing_pool),
        99,
        "distributable"
    );

    // Token movements.
    assert_eq!(test.tokens(FEE_RECIPIENT_TOKEN), FEE);
    assert_eq!(test.tokens(TOKEN_B), STARTING_TOKENS - STAKE_B + PAYOUT_B);
    assert_eq!(test.tokens(TOKEN_A), STARTING_TOKENS - STAKE_A);
    assert_eq!(test.tokens(vault), 0, "vault drained");
}

/// A cancelled event refunds each bettor their exact stake, and once every
/// refund is taken the admin closes the outcomes and the event.
#[quasar_test]
fn cancelled_event_refunds_the_exact_stake(test: &mut Test) {
    base_world(test);
    add_bettor(test, BETTOR_A, TOKEN_A);
    add_bettor(test, BETTOR_B, TOKEN_B);

    let event = test.derive_pda(Event::seeds(EVENT_ID));
    let vault = test.derive_pda(EventVaultPda::seeds(&event));
    let outcome0 = test.derive_pda(Outcome::seeds(&event, 0));
    let outcome1 = test.derive_pda(Outcome::seeds(&event, 1));
    let bet = test.derive_pda(Bet::seeds(&outcome0, &BETTOR_A));
    let bet_b = test.derive_pda(Bet::seeds(&outcome1, &BETTOR_B));

    const STAKE: u64 = 250;

    test.send(InitializeEventInstruction {
        admin: ADMIN,
        token_mint: TOKEN_MINT,
        event_id: EVENT_ID,
        betting_closes_at: BETTING_CLOSES_AT,
        description: "Team A vs Team B".to_string().into(),
    })
    .succeeds();
    test.send(AddOutcomeInstruction {
        admin: ADMIN,
        event_event_id_seed: EVENT_ID,
        event_outcome_count_seed: 0,
        label: "Team A".to_string().into(),
    })
    .succeeds();
    test.send(AddOutcomeInstruction {
        admin: ADMIN,
        event_event_id_seed: EVENT_ID,
        event_outcome_count_seed: 1,
        label: "Team B".to_string().into(),
    })
    .succeeds();
    test.send(OpenBettingInstruction {
        admin: ADMIN,
        event_event_id_seed: EVENT_ID,
    })
    .succeeds();
    test.send(PlaceBetInstruction {
        bettor: BETTOR_A,
        token_mint: TOKEN_MINT,
        event_event_id_seed: EVENT_ID,
        outcome_index_seed: 0,
        bettor_token_account: TOKEN_A,
        amount: STAKE,
    })
    .succeeds();
    test.send(bet_on(BETTOR_B, TOKEN_B, 1, STAKE)).succeeds();
    test.send(CancelEventInstruction {
        admin: ADMIN,
        event_event_id_seed: EVENT_ID,
    })
    .succeeds();
    test.send(ClaimRefundInstruction {
        bettor: BETTOR_A,
        token_mint: TOKEN_MINT,
        event_event_id_seed: EVENT_ID,
        bet,
        bettor_token_account: TOKEN_A,
    })
    .succeeds()
    .is_closed(bet);

    assert_eq!(
        test.read::<Event>(event).status,
        EventStatus::Cancelled as u8
    );
    // The bettor got their exact stake back and the bet closed.
    assert_eq!(test.tokens(TOKEN_A), STARTING_TOKENS);

    // B's refund is still outstanding, so the event's accounts stay.
    test.send(close_outcome(ADMIN, 0))
        .fails_with(BettingError::BetsStillOpen);
    assert_eq!(u64::from(test.read::<Event>(event).open_bets), 1);

    test.send(ClaimRefundInstruction {
        bettor: BETTOR_B,
        token_mint: TOKEN_MINT,
        event_event_id_seed: EVENT_ID,
        bet: bet_b,
        bettor_token_account: TOKEN_B,
    })
    .succeeds()
    .is_closed(bet_b);
    assert_eq!(test.tokens(TOKEN_B), STARTING_TOKENS);
    assert_eq!(test.tokens(vault), 0);

    // With every stake refunded the admin closes the outcomes and the event.
    // The vault was empty, so the fee recipient receives nothing.
    assert_eq!(u64::from(test.read::<Event>(event).open_bets), 0);
    let vault_balance_at_close = close_outcomes_and_event(test, 2);
    assert_eq!(vault_balance_at_close, 0);
    assert_eq!(test.tokens(FEE_RECIPIENT_TOKEN), 0);
}

/// Only the config admin may create an event, or add an outcome to one.
#[quasar_test]
fn initialize_event_rejects_a_non_admin_signer(test: &mut Test) {
    base_world(test);
    test.add(Wallet::new().at(ATTACKER));

    test.send(InitializeEventInstruction {
        admin: ATTACKER,
        token_mint: TOKEN_MINT,
        event_id: EVENT_ID,
        betting_closes_at: BETTING_CLOSES_AT,
        description: "Team A vs Team B".to_string().into(),
    })
    .fails_with(BettingError::Unauthorized);

    // Nor can anyone but the admin add an outcome to the admin's draft.
    draft_event(test, &["Glowbugs 3", "The Quiet Floor"]);
    test.send(AddOutcomeInstruction {
        admin: ATTACKER,
        event_event_id_seed: EVENT_ID,
        event_outcome_count_seed: 2,
        label: "Late entry".to_string().into(),
    })
    .fails_with(BettingError::Unauthorized);
}

/// Create the event and add `labels` as its outcomes, leaving it a draft.
fn draft_event(test: &mut Test, labels: &[&str]) {
    test.send(InitializeEventInstruction {
        admin: ADMIN,
        token_mint: TOKEN_MINT,
        event_id: EVENT_ID,
        betting_closes_at: BETTING_CLOSES_AT,
        description: "Top-grossing film".to_string().into(),
    })
    .succeeds();
    for (index, label) in labels.iter().enumerate() {
        test.send(AddOutcomeInstruction {
            admin: ADMIN,
            event_event_id_seed: EVENT_ID,
            event_outcome_count_seed: index as u8,
            label: label.to_string().into(),
        })
        .succeeds();
    }
}

/// Give `bettor` a funded token account at `token_account`.
fn add_bettor(test: &mut Test, bettor: Pubkey, token_account: Pubkey) {
    test.add(Wallet::new().at(bettor));
    test.add(
        TokenAccount::new(TOKEN_MINT, bettor)
            .at(token_account)
            .amount(STARTING_TOKENS),
    );
}

fn bet(bettor: Pubkey, token_account: Pubkey, outcome: u8, amount: u64) -> PlaceBetInstruction {
    bet_on(bettor, token_account, outcome, amount)
}

/// The same as `bet`, for tests whose local `bet` is an address.
fn bet_on(bettor: Pubkey, token_account: Pubkey, outcome: u8, amount: u64) -> PlaceBetInstruction {
    PlaceBetInstruction {
        bettor,
        token_mint: TOKEN_MINT,
        event_event_id_seed: EVENT_ID,
        outcome_index_seed: outcome,
        bettor_token_account: token_account,
        amount,
    }
}

fn close_outcome(admin: Pubkey, outcome_index: u8) -> CloseOutcomeInstruction {
    CloseOutcomeInstruction {
        admin,
        event_event_id_seed: EVENT_ID,
        outcome_index_seed: outcome_index,
    }
}

fn close_event(admin: Pubkey) -> CloseEventInstruction {
    CloseEventInstruction {
        admin,
        token_mint: TOKEN_MINT,
        event_event_id_seed: EVENT_ID,
        fee_recipient_token_account: FEE_RECIPIENT_TOKEN,
    }
}

/// Closes a finished event's Outcome accounts and then the event itself, as
/// the admin, asserting that each rent comes back to the admin and that every
/// closed account is gone. quasar-test charges no transaction fee, so the
/// admin's balance rises by exactly each account's rent. Returns what the
/// vault held before it closed.
fn close_outcomes_and_event(test: &mut Test, outcome_count: u8) -> u64 {
    let event = test.derive_pda(Event::seeds(EVENT_ID));
    let vault = test.derive_pda(EventVaultPda::seeds(&event));

    for index in 0..outcome_count {
        let outcome = test.derive_pda(Outcome::seeds(&event, index));
        let outcome_rent = test.lamports(outcome);
        let admin_before = test.lamports(ADMIN);
        test.send(close_outcome(ADMIN, index))
            .succeeds()
            .is_closed(outcome)
            .has_lamports(ADMIN, admin_before + outcome_rent);
    }
    assert_eq!(test.read::<Event>(event).open_outcomes, 0);

    let vault_balance = test.tokens(vault);
    let event_rent = test.lamports(event);
    let vault_rent = test.lamports(vault);
    let admin_before = test.lamports(ADMIN);
    test.send(close_event(ADMIN))
        .succeeds()
        .is_closed(event)
        .is_closed(vault)
        .has_lamports(ADMIN, admin_before + event_rent + vault_rent);
    vault_balance
}

fn settle(winning_outcome_index: u8) -> SettleEventInstruction {
    SettleEventInstruction {
        admin: ADMIN,
        token_mint: TOKEN_MINT,
        event_event_id_seed: EVENT_ID,
        fee_recipient_token_account: FEE_RECIPIENT_TOKEN,
        winning_outcome_index,
    }
}

fn open_betting(admin: Pubkey) -> OpenBettingInstruction {
    OpenBettingInstruction {
        admin,
        event_event_id_seed: EVENT_ID,
    }
}

/// The outcome list is final once betting opens, and nobody can bet before it
/// is. A bet on a draft would otherwise let anyone freeze a half-built market,
/// and an outcome added after a bet would change the question under it.
#[quasar_test]
fn outcomes_lock_when_betting_opens(test: &mut Test) {
    base_world(test);
    add_bettor(test, BETTOR_A, TOKEN_A);
    draft_event(test, &["Glowbugs 3"]);

    test.send(bet(BETTOR_A, TOKEN_A, 0, 100))
        .fails_with(BettingError::EventNotOpen);

    test.send(AddOutcomeInstruction {
        admin: ADMIN,
        event_event_id_seed: EVENT_ID,
        event_outcome_count_seed: 1,
        label: "The Quiet Floor".to_string().into(),
    })
    .succeeds();
    test.send(open_betting(ADMIN)).succeeds();

    test.send(AddOutcomeInstruction {
        admin: ADMIN,
        event_event_id_seed: EVENT_ID,
        event_outcome_count_seed: 2,
        label: "Late entry".to_string().into(),
    })
    .fails_with(BettingError::EventNotDraft);
    test.send(open_betting(ADMIN))
        .fails_with(BettingError::EventNotDraft);
}

/// A market needs a losing side, and only the admin can open it.
#[quasar_test]
fn open_betting_needs_two_outcomes_and_the_admin(test: &mut Test) {
    base_world(test);
    test.add(Wallet::new().at(ATTACKER));
    draft_event(test, &["The only horse"]);

    test.send(open_betting(ADMIN))
        .fails_with(BettingError::NotEnoughOutcomes);

    test.send(AddOutcomeInstruction {
        admin: ADMIN,
        event_event_id_seed: EVENT_ID,
        event_outcome_count_seed: 1,
        label: "A second horse".to_string().into(),
    })
    .succeeds();
    test.send(open_betting(ATTACKER))
        .fails_with(BettingError::Unauthorized);
    test.send(open_betting(ADMIN)).succeeds();
    assert_eq!(
        test.read::<Event>(test.derive_pda(Event::seeds(EVENT_ID)))
            .status,
        EventStatus::Open as u8
    );
}

/// Bets land strictly before the close time and settlement only at or after
/// it, so there is no second at which someone who knows the result can still
/// stake, and none at which the admin can end the market early.
#[quasar_test]
fn betting_closes_at_the_close_time(test: &mut Test) {
    base_world(test);
    add_bettor(test, BETTOR_A, TOKEN_A);
    add_bettor(test, BETTOR_B, TOKEN_B);
    draft_event(test, &["Home", "Away"]);
    test.send(open_betting(ADMIN)).succeeds();

    test.warp_to_timestamp(BETTING_CLOSES_AT - 1);
    test.send(bet(BETTOR_A, TOKEN_A, 0, 100)).succeeds();
    test.send(settle(0))
        .fails_with(BettingError::BettingStillOpen);

    test.warp_to_timestamp(BETTING_CLOSES_AT);
    test.send(bet(BETTOR_B, TOKEN_B, 1, 100))
        .fails_with(BettingError::BettingClosed);
    test.send(settle(0)).succeeds();
}

#[quasar_test]
fn initialize_event_rejects_a_close_time_in_the_past(test: &mut Test) {
    base_world(test);

    test.send(InitializeEventInstruction {
        admin: ADMIN,
        token_mint: TOKEN_MINT,
        event_id: EVENT_ID,
        betting_closes_at: START_TIME,
        description: "Already over".to_string().into(),
    })
    .fails_with(BettingError::CloseTimeInPast);
}

/// A wallet can hold as many open positions as it likes: forty bets across
/// forty outcomes all land. The program keeps no per-wallet index of open bets,
/// so there is no list to fill.
#[quasar_test]
fn no_cap_on_open_bets_per_wallet(test: &mut Test) {
    const OUTCOME_COUNT: u8 = 40;
    const STAKE: u64 = 10;
    base_world(test);
    add_bettor(test, BETTOR_A, TOKEN_A);
    let labels: Vec<String> = (0..OUTCOME_COUNT)
        .map(|index| format!("Runner {index}"))
        .collect();
    let label_refs: Vec<&str> = labels.iter().map(String::as_str).collect();
    draft_event(test, &label_refs);
    test.send(open_betting(ADMIN)).succeeds();

    let event = test.derive_pda(Event::seeds(EVENT_ID));
    for index in 0..OUTCOME_COUNT {
        test.send(bet(BETTOR_A, TOKEN_A, index, STAKE)).succeeds();
        let outcome = test.derive_pda(Outcome::seeds(&event, index));
        let bet_state = test.read::<Bet>(test.derive_pda(Bet::seeds(&outcome, &BETTOR_A)));
        assert_eq!(bet_state.bettor, BETTOR_A, "bet {index} records its bettor");
    }
    assert_eq!(
        test.tokens(TOKEN_A),
        STARTING_TOKENS - OUTCOME_COUNT as u64 * STAKE
    );
}

/// The whole lifecycle through to every account closing. Stakes are chosen so
/// the pro-rata split does not divide evenly: outcome 0 pool 300 (A 100 in two
/// bets, B 200), outcome 1 pool 250 (C). Outcome 0 wins: losing pool 250, fee
/// 2 (1%), distributable 248; A gets floor(100 * 248 / 300) = 82 and B
/// floor(200 * 248 / 300) = 165, so one minor unit of dust stays in the vault
/// until `close_event` pays it to the fee recipient.
#[quasar_test]
fn close_event_pays_dust_to_fee_recipient_and_returns_rent(test: &mut Test) {
    base_world(test);
    add_bettor(test, BETTOR_A, TOKEN_A);
    add_bettor(test, BETTOR_B, TOKEN_B);
    add_bettor(test, BETTOR_C, TOKEN_C);
    draft_event(test, &["Yes", "No"]);
    test.send(open_betting(ADMIN)).succeeds();

    let event = test.derive_pda(Event::seeds(EVENT_ID));
    let vault = test.derive_pda(EventVaultPda::seeds(&event));
    let outcome0 = test.derive_pda(Outcome::seeds(&event, 0));
    let outcome1 = test.derive_pda(Outcome::seeds(&event, 1));
    let bet_a = test.derive_pda(Bet::seeds(&outcome0, &BETTOR_A));
    let bet_b = test.derive_pda(Bet::seeds(&outcome0, &BETTOR_B));
    let bet_c = test.derive_pda(Bet::seeds(&outcome1, &BETTOR_C));

    // A's second bet tops up the existing Bet account rather than creating
    // another, so it counts once among the open bets.
    test.send(bet(BETTOR_A, TOKEN_A, 0, 60)).succeeds();
    test.send(bet(BETTOR_A, TOKEN_A, 0, 40)).succeeds();
    test.send(bet(BETTOR_B, TOKEN_B, 0, 200)).succeeds();
    test.send(bet(BETTOR_C, TOKEN_C, 1, 250)).succeeds();
    let event_state = test.read::<Event>(event);
    assert_eq!(u64::from(event_state.open_bets), 3);
    assert_eq!(event_state.open_outcomes, 2);
    assert_eq!(u64::from(event_state.total_pool), 550);

    test.warp_to_timestamp(BETTING_CLOSES_AT);
    test.send(settle(0)).succeeds();
    assert_eq!(test.tokens(FEE_RECIPIENT_TOKEN), 2);

    // C closes the losing bet; A and B claim.
    test.send(CloseLosingBetInstruction {
        bettor: BETTOR_C,
        event_event_id_seed: EVENT_ID,
        bet: bet_c,
    })
    .succeeds()
    .is_closed(bet_c);
    assert_eq!(u64::from(test.read::<Event>(event).open_bets), 2);
    for (winner, winner_token, winner_bet) in
        [(BETTOR_A, TOKEN_A, bet_a), (BETTOR_B, TOKEN_B, bet_b)]
    {
        test.send(ClaimWinningsInstruction {
            bettor: winner,
            token_mint: TOKEN_MINT,
            event_event_id_seed: EVENT_ID,
            bet: winner_bet,
            bettor_token_account: winner_token,
        })
        .succeeds()
        .is_closed(winner_bet);
    }
    assert_eq!(test.tokens(TOKEN_A), STARTING_TOKENS - 100 + 182);
    assert_eq!(test.tokens(TOKEN_B), STARTING_TOKENS - 200 + 365);
    assert_eq!(u64::from(test.read::<Event>(event).open_bets), 0);

    // The two floors left one minor unit in the vault.
    assert_eq!(test.tokens(vault), 1);

    // Outcome accounts close before the event does.
    test.send(close_event(ADMIN))
        .fails_with(BettingError::OutcomesStillOpen);

    let vault_balance_at_close = close_outcomes_and_event(test, 2);
    assert_eq!(vault_balance_at_close, 1);
    // The dust joined the fee: 2 + 1.
    assert_eq!(test.tokens(FEE_RECIPIENT_TOKEN), 3);
}

/// A settled event keeps its accounts while any Bet account of it is open:
/// the claim and the losing-bet close both read the event and the outcome.
#[quasar_test]
fn close_event_refused_while_a_bet_is_open(test: &mut Test) {
    base_world(test);
    add_bettor(test, BETTOR_A, TOKEN_A);
    add_bettor(test, BETTOR_C, TOKEN_C);
    draft_event(test, &["Yes", "No"]);
    test.send(open_betting(ADMIN)).succeeds();

    let event = test.derive_pda(Event::seeds(EVENT_ID));
    let outcome0 = test.derive_pda(Outcome::seeds(&event, 0));
    let outcome1 = test.derive_pda(Outcome::seeds(&event, 1));
    let bet_a = test.derive_pda(Bet::seeds(&outcome0, &BETTOR_A));
    let bet_c = test.derive_pda(Bet::seeds(&outcome1, &BETTOR_C));

    test.send(bet(BETTOR_A, TOKEN_A, 0, 100)).succeeds();
    test.send(bet(BETTOR_C, TOKEN_C, 1, 300)).succeeds();
    test.warp_to_timestamp(BETTING_CLOSES_AT);
    test.send(settle(0)).succeeds();

    // A has claimed; C's losing bet is still open.
    test.send(ClaimWinningsInstruction {
        bettor: BETTOR_A,
        token_mint: TOKEN_MINT,
        event_event_id_seed: EVENT_ID,
        bet: bet_a,
        bettor_token_account: TOKEN_A,
    })
    .succeeds()
    .is_closed(bet_a);
    assert_eq!(u64::from(test.read::<Event>(event).open_bets), 1);

    test.send(close_event(ADMIN))
        .fails_with(BettingError::BetsStillOpen);
    test.send(close_outcome(ADMIN, 1))
        .fails_with(BettingError::BetsStillOpen);

    // Once C closes the bet, everything else can close.
    test.send(CloseLosingBetInstruction {
        bettor: BETTOR_C,
        event_event_id_seed: EVENT_ID,
        bet: bet_c,
    })
    .succeeds()
    .is_closed(bet_c);
    close_outcomes_and_event(test, 2);
}

/// A draft or open event has not finished, so its accounts cannot close,
/// even when nobody has bet. Cancelling it is what lets it close.
#[quasar_test]
fn close_event_refused_while_event_is_open(test: &mut Test) {
    base_world(test);
    draft_event(test, &["Yes", "No"]);

    // As a draft...
    test.send(close_event(ADMIN))
        .fails_with(BettingError::EventNotFinished);
    test.send(close_outcome(ADMIN, 0))
        .fails_with(BettingError::EventNotFinished);

    // ...and once open.
    test.send(open_betting(ADMIN)).succeeds();
    test.send(close_event(ADMIN))
        .fails_with(BettingError::EventNotFinished);

    // Cancelled with no bets, it closes straight away.
    test.send(CancelEventInstruction {
        admin: ADMIN,
        event_event_id_seed: EVENT_ID,
    })
    .succeeds();
    let vault_balance_at_close = close_outcomes_and_event(test, 2);
    assert_eq!(vault_balance_at_close, 0);
}

#[quasar_test]
fn only_admin_can_close_outcomes_and_event(test: &mut Test) {
    base_world(test);
    test.add(Wallet::new().at(ATTACKER));
    add_bettor(test, BETTOR_A, TOKEN_A);
    add_bettor(test, BETTOR_C, TOKEN_C);
    draft_event(test, &["Yes", "No"]);
    test.send(open_betting(ADMIN)).succeeds();

    let event = test.derive_pda(Event::seeds(EVENT_ID));
    let outcome0 = test.derive_pda(Outcome::seeds(&event, 0));
    let outcome1 = test.derive_pda(Outcome::seeds(&event, 1));
    let bet_a = test.derive_pda(Bet::seeds(&outcome0, &BETTOR_A));
    let bet_c = test.derive_pda(Bet::seeds(&outcome1, &BETTOR_C));

    test.send(bet(BETTOR_A, TOKEN_A, 0, 100)).succeeds();
    test.send(bet(BETTOR_C, TOKEN_C, 1, 300)).succeeds();
    test.warp_to_timestamp(BETTING_CLOSES_AT);
    test.send(settle(0)).succeeds();
    test.send(ClaimWinningsInstruction {
        bettor: BETTOR_A,
        token_mint: TOKEN_MINT,
        event_event_id_seed: EVENT_ID,
        bet: bet_a,
        bettor_token_account: TOKEN_A,
    })
    .succeeds();
    test.send(CloseLosingBetInstruction {
        bettor: BETTOR_C,
        event_event_id_seed: EVENT_ID,
        bet: bet_c,
    })
    .succeeds();

    // Every bet is closed, so only the signer stands between the attacker and
    // the rent.
    test.send(close_outcome(ATTACKER, 0))
        .fails_with(BettingError::Unauthorized);
    test.send(close_event(ATTACKER))
        .fails_with(BettingError::Unauthorized);
    assert!(test.lamports(event) > 0, "the event stays open");
    assert!(test.lamports(outcome0) > 0, "the outcome stays open");

    close_outcomes_and_event(test, 2);
}
