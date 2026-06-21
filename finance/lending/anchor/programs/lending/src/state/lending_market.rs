use anchor_lang::prelude::*;

/// Top-level configuration shared by every reserve and obligation under it.
/// The owner is the only account that may create reserves and change their config.
#[account]
#[derive(InitSpace)]
pub struct LendingMarket {
    /// Index this market's PDA is derived from (`["lending_market", market_id]`).
    /// The market is identified by this id, not by any individual — `owner` below
    /// is a stored field used only for authorization, never part of the address.
    /// Distinct markets (0, 1, 2 …) give independent, risk-isolated pools.
    pub market_id: u64,

    pub owner: Pubkey,

    /// The mint that obligation values are denominated in (for example USDC).
    /// Stored for reference; valuations come from each reserve's own price feed,
    /// which must report prices in this currency.
    pub quote_currency_mint: Pubkey,

    pub bump: u8,
}
