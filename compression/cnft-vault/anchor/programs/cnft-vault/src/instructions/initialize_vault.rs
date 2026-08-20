use anchor_lang::prelude::*;

use crate::state::{Vault, VAULT_SEED};

#[derive(Accounts)]
pub struct InitializeVaultAccountConstraints {
    #[account(mut)]
    pub authority: Signer,

    #[account(
        init,
        payer = authority,
        space = Vault::DISCRIMINATOR.len() + Vault::INIT_SPACE,
        seeds = [VAULT_SEED],
        bump,
    )]
    pub vault: BorshAccount<Vault>,

    pub system_program: Program<System>,
}

pub fn handler(context: &mut Context<InitializeVaultAccountConstraints>) -> Result<()> {
    let vault = &mut context.accounts.vault;
    vault.authority = *context.accounts.authority.address();
    vault.bump = context.bumps.vault;
    Ok(())
}
