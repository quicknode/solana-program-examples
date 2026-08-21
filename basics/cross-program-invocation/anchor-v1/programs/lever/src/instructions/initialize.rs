use anchor_lang::prelude::*;

use crate::PowerStatus;

#[derive(Accounts)]
pub struct InitializeLeverAccountConstraints<'info> {
    #[account(init, payer = user, space = PowerStatus::DISCRIMINATOR.len() + PowerStatus::INIT_SPACE)]
    pub power: Account<'info, PowerStatus>,
    #[account(mut)]
    pub user: Signer<'info>,
    pub system_program: Program<'info, System>,
}

pub fn handler(_context: Context<InitializeLeverAccountConstraints>) -> Result<()> {
    Ok(())
}
