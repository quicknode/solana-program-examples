use anchor_lang::prelude::*;
use anchor_spl::token_interface::{Mint, TokenInterface};

use crate::state::RouterConfig;

#[derive(Accounts)]
pub struct InitializeRouterAccountConstraints {
    #[account(mut)]
    pub authority: Signer,

    pub usdc_mint: InterfaceAccount<Mint>,

    #[account(
        init,
        payer = authority,
        space = RouterConfig::DISCRIMINATOR.len() + RouterConfig::INIT_SPACE,
        seeds = [b"router_config"],
        bump
    )]
    pub router_config: BorshAccount<RouterConfig>,

    /// CHECK: PDA used as mint authority only - no data stored
    #[account(
        seeds = [b"router_authority"],
        bump
    )]
    pub router_authority: UncheckedAccount,

    pub token_program: Interface<'static, TokenInterface>,
    pub system_program: Program<System>,
}

pub fn handle_initialize_router(
    context: &mut Context<InitializeRouterAccountConstraints>,
    _usdc_mint: Address,
) -> Result<()> {
    *context.accounts.router_config = RouterConfig {
        authority: *context.accounts.authority.address(),
        usdc_mint: *context.accounts.usdc_mint.address(),
        bump: context.bumps.router_config,
    };
    Ok(())
}
