use anchor_lang::prelude::*;

use crate::state::Registry;

#[derive(Accounts)]
pub struct InitializeRegistryAccountConstraints {
    #[account(mut)]
    pub authority: Signer,

    #[account(
        init,
        payer = authority,
        space = Registry::DISCRIMINATOR.len() + Registry::INIT_SPACE,
        seeds = [b"registry", authority.address().as_ref()],
        bump
    )]
    pub registry: BorshAccount<Registry>,

    pub system_program: Program<System>,
}

pub fn handle_initialize_registry(
    context: &mut Context<InitializeRegistryAccountConstraints>,
) -> Result<()> {
    *context.accounts.registry = (Registry {
        authority: *context.accounts.authority.address(),
        bump: context.bumps.registry,
    });
    Ok(())
}
