use anchor_lang::prelude::*;

// A bettor can hold at most this many distinct bets (one per outcome they back).
// Re-betting an outcome adds to the existing Bet, so this caps the number of
// *different* outcomes a user has staked on, not the number of times they bet.
// A fixed cap keeps the account a constant size — no reallocation on each bet.
pub const MAX_BETS_PER_USER: usize = 32;

// Per-wallet index of a bettor's bets, so a client can list someone's positions
// without scanning every Bet account on the program. The authoritative stake
// state lives in the Bet accounts; this is a convenience index.
#[account]
#[derive(InitSpace)]
pub struct User {
    pub authority: Pubkey,
    #[max_len(MAX_BETS_PER_USER)]
    pub bets: Vec<Pubkey>,
    pub bump: u8,
}
