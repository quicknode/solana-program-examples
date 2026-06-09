use anchor_lang::prelude::*;

// A single bettor's total stake on one outcome. Re-betting the same outcome
// adds to `amount` rather than creating a second account, so there is exactly
// one Bet per (outcome, bettor).
#[account]
#[derive(InitSpace)]
pub struct Bet {
    pub bettor: Pubkey,
    pub event: Pubkey,
    pub outcome: Pubkey,
    pub outcome_index: u8,
    pub amount: u64,
    pub claimed: bool,
    pub bump: u8,
}
