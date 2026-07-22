use anchor_lang::prelude::*;

// The global, single Config account. Its `admin` is the only key allowed to
// create events, add outcomes, settle, and cancel. `token_mint` fixes the one
// asset every market in this deployment accepts as a stake.
#[account]
#[derive(InitSpace)]
pub struct Config {
    pub admin: Pubkey,
    pub token_mint: Pubkey,
    pub fee_recipient: Pubkey,
    // Protocol fee, in basis points, that new events copy into their own
    // `fee_bps` at creation. Settlement charges the event's copy, so changing
    // this value only affects events created afterwards.
    pub default_fee_bps: u16,
    pub event_count: u64,
    pub bump: u8,
}
