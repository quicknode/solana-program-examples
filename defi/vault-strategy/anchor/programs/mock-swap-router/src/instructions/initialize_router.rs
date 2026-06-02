use anchor_lang::prelude::*;
use anchor_spl::token_interface::{Mint, TokenInterface};

use crate::state::RouterConfig;

#[derive(Accounts)]
pub struct InitializeRouterAccountConstraints<'info> {
    #[account(mut)]
    pub authority: Signer<'info>,

    pub usdc_mint: InterfaceAccount<'info, Mint>,

    #[account(
        init,
        payer = authority,
        space = RouterConfig::DISCRIMINATOR.len() + RouterConfig::INIT_SPACE,
        seeds = [b"router_config"],
        bump
    )]
    pub router_config: Account<'info, RouterConfig>,

    /// CHECK: PDA used as mint authority only — no data stored
    #[account(
        seeds = [b"router_authority"],
        bump
    )]
    pub router_authority: UncheckedAccount<'info>,

    pub token_program: Interface<'info, TokenInterface>,
    pub system_program: Program<'info, System>,
}

pub fn handle_initialize_router(
    context: Context<InitializeRouterAccountConstraints>,
    _usdc_mint: Pubkey,
) -> Result<()> {
    context.accounts.router_config.set_inner(RouterConfig {
        authority: context.accounts.authority.key(),
        usdc_mint: context.accounts.usdc_mint.key(),
        bump: context.bumps.router_config,
    });
    Ok(())
}
