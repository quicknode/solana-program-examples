use anchor_lang::prelude::*;

// One possible result of an event (e.g. "Yes", "Team A wins"). `total_amount`
// is this outcome's share of the pool and is the denominator for pro-rata
// payouts when this outcome wins.
#[account(borsh)]
#[derive(InitSpace)]
pub struct Outcome {
    pub event: Address,
    pub index: u8,
    #[max_len(64)]
    pub label: String,
    pub total_amount: u64,
    pub bet_count: u64,
    pub bump: u8,
}
