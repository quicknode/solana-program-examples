use anchor_lang::prelude::*;

// The global, single Config account. Its `admin` is the only key allowed to
// create events, add outcomes, settle, and cancel. `token_mint` fixes the one
// asset every market in this deployment accepts as a stake.
#[account(borsh)]
#[derive(InitSpace)]
pub struct Config {
    pub admin: Address,
    pub token_mint: Address,
    pub fee_recipient: Address,
    // Protocol fee, in basis points, taken from the losing pool at settlement.
    pub fee_bps: u16,
    pub event_count: u64,
    pub bump: u8,
}
