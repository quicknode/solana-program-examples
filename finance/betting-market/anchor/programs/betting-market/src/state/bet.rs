use anchor_lang::prelude::*;

// A single bettor's total stake on one outcome. Re-betting the same outcome
// adds to `amount` rather than creating a second account, so there is exactly
// one Bet per (outcome, bettor). The account lives only while the position is
// open: it closes (rent back to the bettor) on claim_winnings, claim_refund,
// or close_losing_bet, which is also what prevents double claims.
//
// `bettor` is the first field, right after the 8-byte discriminator, so a
// client lists a wallet's open positions with getProgramAccounts and a memcmp
// filter on the wallet's address at offset 8. The program keeps no per-wallet
// index, so there is no limit on how many positions a wallet holds.
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
