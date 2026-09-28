use anchor_lang::prelude::*;

#[account]
#[derive(InitSpace)]
pub struct RouterConfig {
    pub authority: Pubkey,
    pub usdc_mint: Pubkey,
    pub bump: u8,
}
