use anchor_lang::prelude::*;
use anchor_spl::{
    associated_token::AssociatedToken,
    token_interface::{Mint, TokenAccount, TokenInterface},
};

use crate::state::{AssetRate, RouterConfig};

#[derive(Accounts)]
pub struct SetRateAccountConstraints<'info> {
    #[account(mut)]
    pub authority: Signer<'info>,

    #[account(
        has_one = authority,
        seeds = [b"router_config"],
        bump = router_config.bump
    )]
    pub router_config: Account<'info, RouterConfig>,

    pub asset_mint: InterfaceAccount<'info, Mint>,

    pub usdc_mint: InterfaceAccount<'info, Mint>,

    #[account(
        init_if_needed,
        payer = authority,
        space = AssetRate::DISCRIMINATOR.len() + AssetRate::INIT_SPACE,
        seeds = [b"rate", asset_mint.key().as_ref()],
        bump
    )]
    pub asset_rate: Account<'info, AssetRate>,

    /// CHECK: PDA used as mint authority only
    #[account(
        seeds = [b"router_authority"],
        bump
    )]
    pub router_authority: UncheckedAccount<'info>,

    #[account(
        init_if_needed,
        payer = authority,
        associated_token::mint = usdc_mint,
        associated_token::authority = router_authority,
        associated_token::token_program = token_program
    )]
    pub router_usdc_treasury: InterfaceAccount<'info, TokenAccount>,

    pub associated_token_program: Program<'info, AssociatedToken>,
    pub token_program: Interface<'info, TokenInterface>,
    pub system_program: Program<'info, System>,
}

pub fn handle_set_rate(
    context: Context<SetRateAccountConstraints>,
    _mint: Pubkey,
    usdc_per_token: u64,
) -> Result<()> {
    context.accounts.asset_rate.set_inner(AssetRate {
        mint: context.accounts.asset_mint.key(),
        usdc_per_token,
        bump: context.bumps.asset_rate,
    });
    Ok(())
}
