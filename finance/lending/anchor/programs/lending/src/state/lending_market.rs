use anchor_lang::prelude::*;

/// Top-level configuration shared by every reserve and obligation under it.
/// The owner is the only account that may create reserves and change their config.
#[account]
#[derive(InitSpace)]
pub struct LendingMarket {
    /// Per-owner index this market's PDA is derived from. Seeding by
    /// `(owner, market_id)` rather than the owner alone lets one owner run
    /// several independent, risk-isolated markets ("their market 0, 1, 2 …")
    /// while keeping each owner's index space free of cross-owner collisions.
    pub market_id: u64,

    pub owner: Pubkey,

    /// The mint that obligation values are denominated in (for example USDC).
    /// Stored for reference; valuations come from each reserve's own price feed,
    /// which must report prices in this currency.
    pub quote_currency_mint: Pubkey,

    pub bump: u8,
}
