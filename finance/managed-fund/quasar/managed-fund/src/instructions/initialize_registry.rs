use quasar_lang::prelude::*;

use crate::state::{Registry, RegistryInner};

#[derive(Accounts)]
pub struct InitializeRegistryAccountConstraints {
    #[account(mut)]
    pub authority: Signer,

    #[account(init, payer = authority, address = Registry::seeds(authority.address()))]
    pub registry: Account<Registry>,

    pub rent: Sysvar<Rent>,
    pub system_program: Program<SystemProgram>,
}

#[inline(always)]
pub fn handle_initialize_registry(
    accounts: &mut InitializeRegistryAccountConstraints,
    bumps: &InitializeRegistryAccountConstraintsBumps,
) -> Result<(), ProgramError> {
    accounts.registry.set_inner(RegistryInner {
        authority: *accounts.authority.address(),
        bump: bumps.registry,
    });
    Ok(())
}
