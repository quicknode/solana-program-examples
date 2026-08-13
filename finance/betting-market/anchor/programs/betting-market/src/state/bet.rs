use anchor_lang::prelude::*;

// A single bettor's total stake on one outcome. Re-betting the same outcome
// adds to `amount` rather than creating a second account, so there is exactly
// one Bet per (outcome, bettor). The account lives only while the position is
// open: it closes (rent back to the bettor) on claim_winnings, claim_refund,
// or close_losing_bet, which is also what prevents double claims.
#[account(borsh)]
#[derive(InitSpace)]
pub struct Bet {
    pub bettor: Address,
    pub event: Address,
    pub outcome: Address,
    pub outcome_index: u8,
    pub amount: u64,
    pub bump: u8,
}
