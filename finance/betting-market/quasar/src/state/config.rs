use quasar_lang::prelude::*;

pub const CONFIG_SEED: &[u8] = b"config";

/// The global, single Config account. Its `admin` is the only key allowed to
/// create events, add outcomes, settle, and cancel. `token_mint` fixes the one
/// asset every market in this deployment accepts as a stake.
///
/// PDA: `["config"]`.
#[account(discriminator = 1, set_inner)]
#[seeds(b"config")]
pub struct Config {
    pub admin: Address,
    pub token_mint: Address,
    pub fee_recipient: Address,
    /// Protocol fee, in basis points, that new events copy into their own
    /// `fee_bps` at creation. Settlement charges the event's copy, so changing
    /// this value only affects events created afterwards.
    pub default_fee_bps: u16,
    pub event_count: u64,
    pub bump: u8,
}

pub fn snapshot_config(config: &Account<Config>) -> ConfigInner {
    ConfigInner {
        admin: config.admin,
        token_mint: config.token_mint,
        fee_recipient: config.fee_recipient,
        default_fee_bps: u16::from(config.default_fee_bps),
        event_count: u64::from(config.event_count),
        bump: config.bump,
    }
}
