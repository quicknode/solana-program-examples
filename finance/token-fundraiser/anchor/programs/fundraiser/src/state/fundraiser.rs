use anchor_lang::prelude::*;

#[account(borsh)]
#[derive(InitSpace)]
pub struct Fundraiser {
    pub maker: Address,
    pub mint_to_raise: Address,
    pub amount_to_raise: u64,
    pub current_amount: u64,
    pub time_started: i64,
    pub duration: u16,
    pub bump: u8,
}
