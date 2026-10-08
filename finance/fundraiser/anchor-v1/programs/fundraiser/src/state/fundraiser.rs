use anchor_lang::prelude::*;

#[account]
#[derive(InitSpace)]
pub struct Fundraiser {
    pub maker: Pubkey,
    pub mint_to_raise: Pubkey,
    pub amount_to_raise: u64,
    pub current_amount: u64,
    pub time_started: i64,
    pub duration: u16,
    /// Set by `check_contributions`. A claimed fundraiser accepts no more
    /// contributions and no second claim, and stays open until every
    /// contribution account written for it has been closed.
    pub claimed: bool,
    /// How many contribution accounts written for this fundraiser are still
    /// open. `close_fundraiser` requires zero, so a new fundraiser at the
    /// same address never starts with contribution accounts from an old one.
    pub open_contributions: u32,
    pub bump: u8,
}
