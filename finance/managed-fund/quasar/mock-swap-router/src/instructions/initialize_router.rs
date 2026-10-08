use quasar_lang::prelude::*;
use quasar_spl::prelude::*;

use crate::state::{RouterConfig, RouterConfigInner};

#[derive(Accounts)]
pub struct InitializeRouterAccountConstraints {
    #[account(mut)]
    pub authority: Signer,

    pub usdc_mint: Account<Mint>,

    #[account(init, payer = authority, address = RouterConfig::seeds())]
    pub router_config: Account<RouterConfig>,

    pub rent: Sysvar<Rent>,
    pub token_program: Program<TokenProgram>,
    pub system_program: Program<SystemProgram>,
}

#[inline(always)]
pub fn handle_initialize_router(
    accounts: &mut InitializeRouterAccountConstraints,
    bumps: &InitializeRouterAccountConstraintsBumps,
) -> Result<(), ProgramError> {
    accounts.router_config.set_inner(RouterConfigInner {
        authority: *accounts.authority.address(),
        usdc_mint: *accounts.usdc_mint.address(),
        bump: bumps.router_config,
    });
    Ok(())
}
