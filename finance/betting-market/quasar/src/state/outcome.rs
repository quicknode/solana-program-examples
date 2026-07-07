use quasar_lang::prelude::*;

pub const OUTCOME_SEED: &[u8] = b"outcome";

/// Max stored label length. Stored as a fixed `[u8; 64]` + `label_len` for the
/// same fixed-size reason as `Event::description`.
pub const MAX_LABEL_LEN: usize = 64;

/// One possible result of an event (e.g. "Yes", "Team A wins"). `total_amount`
/// is this outcome's share of the pool and the denominator for pro-rata payouts
/// when this outcome wins.
///
/// PDA: `["outcome", event, index]`.
#[account(discriminator = 3, set_inner)]
#[seeds(b"outcome", event: Address, index: u8)]
pub struct Outcome {
    pub event: Address,
    pub index: u8,
    pub total_amount: u64,
    pub bet_count: u64,
    pub bump: u8,
    pub label_len: u8,
    pub label: [u8; MAX_LABEL_LEN],
}

pub fn snapshot_outcome(outcome: &Account<Outcome>) -> OutcomeInner {
    OutcomeInner {
        event: outcome.event,
        index: outcome.index,
        total_amount: u64::from(outcome.total_amount),
        bet_count: u64::from(outcome.bet_count),
        bump: outcome.bump,
        label_len: outcome.label_len,
        label: outcome.label,
    }
}
