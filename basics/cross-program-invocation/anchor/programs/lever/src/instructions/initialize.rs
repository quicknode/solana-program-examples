use anchor_lang::prelude::*;

use crate::PowerStatus;

#[derive(Accounts)]
pub struct InitializeLeverAccountConstraints {
    #[account(init, payer = user, space = PowerStatus::DISCRIMINATOR.len() + PowerStatus::INIT_SPACE)]
    pub power: BorshAccount<PowerStatus>,
    #[account(mut)]
    pub user: Signer,
    pub system_program: Program<System>,
}

pub fn handler(_context: &mut Context<InitializeLeverAccountConstraints>) -> Result<()> {
    Ok(())
}
