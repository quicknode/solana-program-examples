use anchor_lang::prelude::*;

/// Top-level configuration shared by every reserve and obligation under it.
/// The owner is the only account that may create reserves and change their config.
#[account]
#[derive(InitSpace)]
pub struct LendingMarket {
    /// Unique id this market's PDA is derived from (a client-chosen `Pubkey`,
    /// typically a fresh keypair). Seeding by an id rather than `owner` lets one
    /// owner run several independent, risk-isolated markets.
    pub market_id: Pubkey,

    pub owner: Pubkey,

    /// The mint that obligation values are denominated in (for example USDC).
    /// Stored for reference; valuations come from each reserve's own price feed,
    /// which must report prices in this currency.
    pub quote_currency_mint: Pubkey,

    pub bump: u8,
}
