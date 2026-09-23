use anchor_lang::prelude::*;

#[derive(AnchorSerialize, AnchorDeserialize, Clone, PartialEq, Eq, InitSpace)]
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
#[account]
#[derive(InitSpace)]
pub struct Event {
    pub event_id: u64,
    #[max_len(200)]
    pub description: String,
    pub outcome_count: u8,
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
