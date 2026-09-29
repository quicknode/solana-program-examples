use anchor_lang::prelude::*;

use crate::state::Registry;

#[derive(Accounts)]
pub struct InitializeRegistryAccountConstraints<'info> {
    #[account(mut)]
    pub authority: Signer<'info>,

    #[account(
        init,
        payer = authority,
        space = Registry::DISCRIMINATOR.len() + Registry::INIT_SPACE,
        seeds = [b"registry", authority.key().as_ref()],
        bump
    )]
    pub registry: Account<'info, Registry>,

    pub system_program: Program<'info, System>,
}

pub fn handle_initialize_registry(
    context: Context<InitializeRegistryAccountConstraints>,
) -> Result<()> {
    context.accounts.registry.set_inner(Registry {
        authority: context.accounts.authority.key(),
        bump: context.bumps.registry,
    });
    Ok(())
}
