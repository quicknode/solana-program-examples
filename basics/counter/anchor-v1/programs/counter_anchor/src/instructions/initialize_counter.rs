use anchor_lang::prelude::*;

use crate::Counter;

#[derive(Accounts)]
pub struct InitializeCounterAccountConstraints<'info> {
    #[account(mut)]
    pub payer: Signer<'info>,

    #[account(
        init,
        space = Counter::DISCRIMINATOR.len() + Counter::INIT_SPACE,
        payer = payer
    )]
    pub counter: Account<'info, Counter>,
    pub system_program: Program<'info, System>,
}

pub fn handler(_context: Context<InitializeCounterAccountConstraints>) -> Result<()> {
    Ok(())
}
