use anchor_lang::prelude::*;
use anchor_spl::token_interface::Mint;

use crate::state::{ApprovedAsset, Registry};

#[derive(Accounts)]
pub struct ApproveAssetAccountConstraints<'info> {
    #[account(mut)]
    pub authority: Signer<'info>,

    #[account(
        has_one = authority,
        seeds = [b"registry", authority.key().as_ref()],
        bump = registry.bump
    )]
    pub registry: Account<'info, Registry>,

    pub asset_mint: InterfaceAccount<'info, Mint>,

    #[account(
        init,
        payer = authority,
        space = ApprovedAsset::DISCRIMINATOR.len() + ApprovedAsset::INIT_SPACE,
        seeds = [b"approved_asset", registry.key().as_ref(), asset_mint.key().as_ref()],
        bump
    )]
    pub approved_asset: Account<'info, ApprovedAsset>,

    pub system_program: Program<'info, System>,
}

pub fn handle_approve_asset(
    context: Context<ApproveAssetAccountConstraints>,
    price_feed: Pubkey,
) -> Result<()> {
    context.accounts.approved_asset.set_inner(ApprovedAsset {
        registry: context.accounts.registry.key(),
        mint: context.accounts.asset_mint.key(),
        price_feed,
        bump: context.bumps.approved_asset,
    });
    Ok(())
}
