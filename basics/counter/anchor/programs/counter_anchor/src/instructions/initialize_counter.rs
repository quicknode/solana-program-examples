use anchor_lang::prelude::*;

use crate::Counter;

#[derive(Accounts)]
pub struct InitializeCounterAccountConstraints {
    #[account(mut)]
    pub payer: Signer,

    #[account(
        init,
        space = Counter::DISCRIMINATOR.len() + Counter::INIT_SPACE,
        payer = payer
    )]
    pub counter: Account<Counter>,
    pub system_program: Program<System>,
}

pub fn handler(_context: &mut Context<InitializeCounterAccountConstraints>) -> Result<()> {
    Ok(())
}
