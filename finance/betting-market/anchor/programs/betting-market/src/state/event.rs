use anchor_lang::prelude::*;

#[derive(Clone, PartialEq, Eq, InitSpace, IdlType, wincode::SchemaRead, wincode::SchemaWrite)]
pub enum EventStatus {
    // Being set up: the admin can add outcomes, and no one can bet yet.
    Draft,
    // The outcome list is final. Bets are accepted until `betting_closes_at`,
    // and the event can be settled from that moment on.
    Open,
    // Resolved to a winning outcome; winners may claim.
    Settled,
    // Abandoned; bettors may reclaim their exact stake.
    Cancelled,
}

// One betting market. All stakes across every outcome live in a single vault
// token account whose authority is this Event PDA, so the program signs payouts
// with the event's seeds.
#[account(borsh)]
#[derive(InitSpace)]
pub struct Event {
    pub event_id: u64,
    #[max_len(200)]
    pub description: String,
    pub outcome_count: u8,
    // How many Outcome accounts of this event are still open. `add_outcome`
    // adds one; `close_outcome` subtracts one. `close_event` requires zero,
    // because an Outcome left behind would carry its stakes into a later
    // event created with the same `event_id`.
    pub open_outcomes: u8,
    // How many Bet accounts across every outcome are still open. `place_bet`
    // adds one when it creates a Bet account (a top-up reuses the account);
    // `claim_winnings`, `claim_refund` and `close_losing_bet` each subtract
    // one when they close one. `close_outcome` and `close_event` require zero.
    pub open_bets: u64,
    // Sum of every stake placed across all outcomes.
    pub total_pool: u64,
    pub status: EventStatus,
    // Unix timestamp at which betting stops, fixed at creation. Bets must land
    // strictly before it and settlement can happen only at or after it, so no
    // one can stake once the result could be known.
    pub betting_closes_at: i64,
    // The fee settlement charges, copied from the config's `default_fee_bps`
    // at creation so later Config changes can't alter a market that bettors
    // have already joined.
    pub fee_bps: u16,
    // Fields below are written at settlement and read at claim time.
    pub winning_outcome_index: u8,
    pub winning_pool: u64,
    pub distributable_losing_pool: u64,
    pub bump: u8,
}

// Bets are accepted while now < betting_closes_at.
pub fn betting_is_open(now: i64, betting_closes_at: i64) -> bool {
    now < betting_closes_at
}

// Settlement is allowed once now >= betting_closes_at: the exact complement of
// `betting_is_open`, so there is no instant at which a bet and a settlement
// could both land.
pub fn may_settle(now: i64, betting_closes_at: i64) -> bool {
    now >= betting_closes_at
}
