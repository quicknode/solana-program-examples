use anchor_lang::prelude::*;

#[account(borsh)]
#[derive(InitSpace)]
pub struct Offer {
    pub id: u64,
    pub maker: Address,
    pub token_mint_a: Address,
    pub token_mint_b: Address,
    pub token_b_wanted_amount: u64,
    pub bump: u8,
}
