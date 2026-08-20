//! quasar-test integration tests. They drive the real program instructions
//! end-to-end: initialize the config, open an event, add outcomes, place bets,
//! settle, and claim, asserting on-chain state and token balances at each step.

use {
    crate::{
        cpi::{
            AddOutcomeInstruction, CancelEventInstruction, ClaimRefundInstruction,
            ClaimWinningsInstruction, CloseLosingBetInstruction, InitializeConfigInstruction,
            InitializeEventInstruction, PlaceBetInstruction, SettleEventInstruction,
        },
        state::{Bet, Config, Event, EventStatus, EventVaultPda, Outcome, User},
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

const FEE_BPS: u16 = 100; // 1%
const DECIMALS: u8 = 6;
const STARTING_TOKENS: u64 = 1_000;
const EVENT_ID: u64 = 1;

/// Register the admin, the stake mint, and the fee recipient's token account,
/// then initialize the config.
fn base_world(test: &mut Test) {
    test.add(Wallet::new().at(ADMIN));
    test.add(Mint::new(ADMIN).at(TOKEN_MINT).decimals(DECIMALS));
    test.add(TokenAccount::new(TOKEN_MINT, FEE_RECIPIENT).at(FEE_RECIPIENT_TOKEN));
    test.send(InitializeConfigInstruction {
        admin: ADMIN,
        token_mint: TOKEN_MINT,
        fee_bps: FEE_BPS,
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
    assert_eq!(u16::from(state.fee_bps), FEE_BPS, "fee_bps");
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
    let user_a = test.derive_pda(User::seeds(&BETTOR_A));
    let user_b = test.derive_pda(User::seeds(&BETTOR_B));

    const STAKE_A: u64 = 100;
    const STAKE_B: u64 = 300;
    const FEE: u64 = 1; // floor(100 * 100 / 10000) = 1
    const PAYOUT_B: u64 = STAKE_B + 99; // stake + winnings(99)

    test.send(InitializeEventInstruction {
        admin: ADMIN,
        token_mint: TOKEN_MINT,
        event_id: EVENT_ID,
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
    test.send(SettleEventInstruction {
        admin: ADMIN,
        token_mint: TOKEN_MINT,
        event_event_id_seed: EVENT_ID,
        fee_recipient_token_account: FEE_RECIPIENT_TOKEN,
        winning_outcome_index: 1,
    })
    .succeeds();
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

    // Both bets closed; user indexes emptied.
    assert_eq!(test.read::<User>(user_a).bet_count, 0);
    assert_eq!(test.read::<User>(user_b).bet_count, 0);
}

/// A cancelled event refunds each bettor their exact stake.
#[quasar_test]
fn cancelled_event_refunds_the_exact_stake(test: &mut Test) {
    base_world(test);
    test.add(Wallet::new().at(BETTOR_A));
    test.add(
        TokenAccount::new(TOKEN_MINT, BETTOR_A)
            .at(TOKEN_A)
            .amount(STARTING_TOKENS),
    );

    let event = test.derive_pda(Event::seeds(EVENT_ID));
    let vault = test.derive_pda(EventVaultPda::seeds(&event));
    let outcome0 = test.derive_pda(Outcome::seeds(&event, 0));
    let bet = test.derive_pda(Bet::seeds(&outcome0, &BETTOR_A));

    const STAKE: u64 = 250;

    test.send(InitializeEventInstruction {
        admin: ADMIN,
        token_mint: TOKEN_MINT,
        event_id: EVENT_ID,
        description: "Team A vs Team B".to_string().into(),
    })
    .succeeds();
    test.send(AddOutcomeInstruction {
        admin: ADMIN,
        event_event_id_seed: EVENT_ID,
        event_outcome_count_seed: 0,
        label: "Only".to_string().into(),
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
    assert_eq!(test.tokens(vault), 0);
}

/// Only the config admin may open an event.
#[quasar_test]
fn initialize_event_rejects_a_non_admin_signer(test: &mut Test) {
    base_world(test);
    test.add(Wallet::new().at(ATTACKER));

    test.send(InitializeEventInstruction {
        admin: ATTACKER,
        token_mint: TOKEN_MINT,
        event_id: EVENT_ID,
        description: "Team A vs Team B".to_string().into(),
    })
    .fails_with(crate::errors::BettingError::Unauthorized);
}
