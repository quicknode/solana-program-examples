use quasar_lang::prelude::*;

pub const EVENT_SEED: &[u8] = b"event";

/// Max stored description length. The Anchor build uses a borsh
/// `String<200>`; this port stores the text in a fixed `[u8; 200]` buffer plus
/// `description_len`, keeping the account fixed-size so every post-creation
/// mutation (place_bet, settle, cancel) is a plain in-place write.
pub const MAX_DESCRIPTION_LEN: usize = 200;

/// Lifecycle of a market. Stored on-chain as a `u8`.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum EventStatus {
    Open = 0,
    Settled = 1,
    Cancelled = 2,
}

/// One betting market. All stakes across every outcome live in a single vault
/// token account whose authority is this Event PDA, so the program signs
/// payouts with the event's seeds.
///
/// PDA: `["event", event_id]`.
#[account(discriminator = 2, set_inner)]
#[seeds(b"event", event_id: u64)]
pub struct Event {
    pub event_id: u64,
    pub outcome_count: u8,
    /// Sum of every stake placed across all outcomes.
    pub total_pool: u64,
    pub status: u8,
    /// Fee snapshot taken at creation, so later Config changes can't alter a
    /// market bettors have already joined.
    pub fee_bps: u16,
    /// Written at settlement, read at claim time.
    pub winning_outcome_index: u8,
    pub winning_pool: u64,
    pub distributable_losing_pool: u64,
    pub bump: u8,
    pub description_len: u8,
    pub description: [u8; MAX_DESCRIPTION_LEN],
}

/// PDA marker for an event's single pool vault: `["vault", event]`. The Anchor
/// build uses an associated token account (ATA) owned by the Event PDA; this
/// port uses a program-derived vault instead, matching how the other Quasar
/// finance examples (lending, perpetual-futures) hold pool funds.
#[derive(Seeds)]
#[seeds(b"vault", event: Address)]
pub struct EventVaultPda;

pub fn snapshot_event(event: &Account<Event>) -> EventInner {
    EventInner {
        event_id: u64::from(event.event_id),
        outcome_count: event.outcome_count,
        total_pool: u64::from(event.total_pool),
        status: event.status,
        fee_bps: u16::from(event.fee_bps),
        winning_outcome_index: event.winning_outcome_index,
        winning_pool: u64::from(event.winning_pool),
        distributable_losing_pool: u64::from(event.distributable_losing_pool),
        bump: event.bump,
        description_len: event.description_len,
        description: event.description,
    }
}
