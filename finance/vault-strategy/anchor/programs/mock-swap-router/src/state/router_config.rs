use anchor_lang::prelude::*;

#[account(borsh)]
#[derive(InitSpace)]
pub struct RouterConfig {
    pub authority: Address,
    pub usdc_mint: Address,
    pub bump: u8,
}
