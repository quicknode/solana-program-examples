use anchor_lang::prelude::*;

#[account]
#[derive(InitSpace)]
pub struct AssetRate {
    pub mint: Pubkey,
    /// USDC base units per token base unit.
    /// e.g. 250 means 1 token base unit = 250 USDC base units
    /// (so 1.0 TSLAx = $250 when both have 6 decimals)
    pub usdc_per_token: u64,
    pub bump: u8,
}
