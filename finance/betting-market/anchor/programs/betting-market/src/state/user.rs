use anchor_lang::prelude::*;

use crate::error::BettingError;

// A bettor can hold at most this many OPEN positions at once (one per outcome
// they currently back). Re-betting an outcome adds to the existing Bet, and
// closing a Bet (claim_winnings, claim_refund, close_losing_bet) removes its
// entry, so this caps concurrent positions, not lifetime bets. A fixed cap
// keeps the account a constant size - no reallocation on each bet.
pub const MAX_BETS_PER_USER: usize = 32;

// Per-wallet index of a bettor's open Bet accounts, so a client can list
// someone's positions without scanning every Bet account on the program. The
// authoritative stake state lives in the Bet accounts; this is a convenience
// index. Entries are added by place_bet and removed whenever the Bet account
// closes.
#[account(borsh)]
#[derive(InitSpace)]
pub struct User {
    pub authority: Address,
    #[max_len(MAX_BETS_PER_USER)]
    pub bets: Vec<Address>,
    pub bump: u8,
}

impl User {
    // Drop a closed Bet's entry from the index. Order is not meaningful, so a
    // swap_remove (move the last entry into the gap) is the cheapest removal.
    pub fn remove_bet(&mut self, bet_key: &Address) -> Result<()> {
        let index = self
            .bets
            .iter()
            .position(|entry| entry == bet_key)
            .ok_or(BettingError::BetNotInUserIndex)?;
        self.bets.swap_remove(index);
        Ok(())
    }
}
