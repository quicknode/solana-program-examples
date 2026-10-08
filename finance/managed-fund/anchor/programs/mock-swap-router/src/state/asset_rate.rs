use anchor_lang::prelude::*;

#[account(borsh)]
#[derive(InitSpace)]
pub struct AssetRate {
    pub mint: Address,
    /// USDC base units per whole token, e.g. 250_000_000 means 1.0 TSLAx = $250
    /// with six-decimal USDC. The swaps scale by the asset mint's decimals, so the
    /// rate means the same thing whatever precision the asset has.
    pub usdc_per_token: u64,
    pub bump: u8,
}
