use anchor_lang::prelude::*;
use anchor_spl::{
    associated_token::AssociatedToken,
    token_interface::{Mint, TokenAccount, TokenInterface},
};

use crate::state::{AssetRate, RouterConfig};

#[derive(Accounts)]
pub struct SetRateAccountConstraints {
    #[account(mut, address = router_config.authority)]
    pub authority: Signer,

    #[account(seeds = [b"router_config"],
        bump = router_config.bump)]
    pub router_config: BorshAccount<RouterConfig>,

    pub asset_mint: InterfaceAccount<Mint>,

    pub usdc_mint: InterfaceAccount<Mint>,

    #[account(
        init_if_needed,
        payer = authority,
        space = AssetRate::DISCRIMINATOR.len() + AssetRate::INIT_SPACE,
        seeds = [b"rate", asset_mint.address().as_ref()],
        bump
    )]
    pub asset_rate: BorshAccount<AssetRate>,

    /// CHECK: PDA used as mint authority only
    #[account(
        seeds = [b"router_authority"],
        bump
    )]
    pub router_authority: UncheckedAccount,

    #[account(
        init_if_needed,
        payer = authority,
        associated_token::mint = usdc_mint,
        associated_token::authority = router_authority,
        associated_token::token_program = token_program
    )]
    pub router_usdc_treasury: InterfaceAccount<TokenAccount>,

    pub associated_token_program: Program<AssociatedToken>,
    pub token_program: Interface<'static, TokenInterface>,
    pub system_program: Program<System>,
}

pub fn handle_set_rate(
    context: &mut Context<SetRateAccountConstraints>,
    _mint: Address,
    usdc_per_token: u64,
) -> Result<()> {
    *context.accounts.asset_rate = (AssetRate {
        mint: *context.accounts.asset_mint.address(),
        usdc_per_token,
        bump: context.bumps.asset_rate,
    });
    Ok(())
}
