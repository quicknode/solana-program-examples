use anchor_lang::prelude::*;

use crate::state::{Vault, VAULT_SEED};

#[derive(Accounts)]
pub struct InitializeVaultAccountConstraints<'info> {
    #[account(mut)]
    pub authority: Signer<'info>,

    #[account(
        init,
        payer = authority,
        space = Vault::DISCRIMINATOR.len() + Vault::INIT_SPACE,
        seeds = [VAULT_SEED],
        bump,
    )]
    pub vault: Account<'info, Vault>,

    pub system_program: Program<'info, System>,
}

pub fn handler(context: Context<InitializeVaultAccountConstraints>) -> Result<()> {
    let vault = &mut context.accounts.vault;
    vault.authority = context.accounts.authority.key();
    vault.bump = context.bumps.vault;
    Ok(())
}
