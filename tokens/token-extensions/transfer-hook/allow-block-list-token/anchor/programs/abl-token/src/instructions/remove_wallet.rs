use anchor_lang::prelude::*;

use crate::{ABWallet, Config};

#[derive(Accounts)]
pub struct RemoveWalletAccountConstraints {
    #[account(mut)]
    pub authority: Signer,

    #[account(
        seeds = [b"config"],
        bump = config.bump,
        has_one = authority,
    )]
    pub config: Box<BorshAccount<Config>>,

    #[account(
        mut,
        close = authority,
    )]
    pub ab_wallet: BorshAccount<ABWallet>,

    pub system_program: Program<System>,
}

impl RemoveWalletAccountConstraints<'_> {
    pub fn remove_wallet(&mut self) -> Result<()> {
        Ok(())
    }
}
