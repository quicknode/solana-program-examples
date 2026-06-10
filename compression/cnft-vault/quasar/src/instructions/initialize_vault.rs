use crate::state::{Vault, VaultInner};
use quasar_lang::prelude::*;

#[derive(Accounts)]
pub struct InitializeVaultAccountConstraints {
    #[account(mut)]
    pub authority: Signer,

    #[account(mut, init, payer = authority, address = Vault::seeds())]
    pub vault: Account<Vault>,

    pub system_program: Program<SystemProgram>,
}

pub fn handle_initialize_vault(
    accounts: &mut InitializeVaultAccountConstraints,
    bump: u8,
) -> Result<(), ProgramError> {
    accounts.vault.set_inner(VaultInner {
        authority: *accounts.authority.address(),
        bump,
    });
    Ok(())
}
