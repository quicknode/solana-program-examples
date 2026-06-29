use anchor_lang::prelude::*;

#[derive(AnchorSerialize, AnchorDeserialize, Clone, PartialEq, Eq, InitSpace)]
pub enum EventStatus {
    // Accepting bets.
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
    // Fee snapshot taken at creation, so later Config changes can't alter a
    // market that bettors have already joined.
    pub fee_bps: u16,
    // Fields below are written at settlement and read at claim time.
    pub winning_outcome_index: u8,
    pub winning_pool: u64,
    pub distributable_losing_pool: u64,
    pub bump: u8,
}
