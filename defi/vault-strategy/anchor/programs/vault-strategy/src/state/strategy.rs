use anchor_lang::prelude::*;

#[account]
#[derive(InitSpace)]
pub struct Strategy {
    pub manager: Pubkey,
    pub share_mint: Pubkey,
    pub usdc_mint: Pubkey,
    pub asset_mint_a: Pubkey,
    pub asset_mint_b: Pubkey,
    /// Allocation weight for asset A in basis points (e.g. 4000 = 40%)
    pub weight_bps_a: u16,
    /// Allocation weight for asset B in basis points (e.g. 6000 = 60%)
    pub weight_bps_b: u16,
    /// Annual management fee in basis points (e.g. 100 = 1%)
    pub fee_bps: u16,
    pub total_shares: u64,
    pub last_fee_accrual_timestamp: i64,
    pub swap_router: Pubkey,
    pub price_feed_a: Pubkey, // Pyth PriceUpdateV2 account for asset_mint_a
    pub price_feed_b: Pubkey, // Pyth PriceUpdateV2 account for asset_mint_b
    pub bump: u8,
}
