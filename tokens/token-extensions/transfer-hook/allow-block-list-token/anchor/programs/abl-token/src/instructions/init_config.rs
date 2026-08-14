use crate::{Config, CONFIG_SEED};
use anchor_lang::prelude::*;

#[derive(Accounts)]
pub struct InitConfigAccountConstraints {
    #[account(mut)]
    pub payer: Signer,

    #[account(
        init,
        payer = payer,
        space = Config::DISCRIMINATOR.len() + Config::INIT_SPACE,
        seeds = [CONFIG_SEED],
        bump,
    )]
    pub config: Box<BorshAccount<Config>>,

    pub system_program: Program<System>,
}

impl InitConfigAccountConstraints {
    pub fn init_config(&mut self, config_bump: u8) -> Result<()> {
        **self.config = Config {
            authority: *self.payer.address(),
            bump: config_bump,
        };

        Ok(())
    }
}
