use anchor_lang::prelude::*;
use anchor_spl::token_interface::Mint;

use crate::state::{ApprovedAsset, Registry};

#[derive(Accounts)]
pub struct ApproveAssetAccountConstraints {
    #[account(mut, address = registry.authority)]
    pub authority: Signer,

    #[account(
        seeds = [b"registry", authority.address().as_ref()],
        bump = registry.bump,
    )]
    pub registry: BorshAccount<Registry>,

    pub asset_mint: InterfaceAccount<Mint>,

    #[account(
        init,
        payer = authority,
        space = ApprovedAsset::DISCRIMINATOR.len() + ApprovedAsset::INIT_SPACE,
        seeds = [b"approved_asset", registry.address().as_ref(), asset_mint.address().as_ref()],
        bump
    )]
    pub approved_asset: BorshAccount<ApprovedAsset>,

    pub system_program: Program<System>,
}

pub fn handle_approve_asset(
    context: &mut Context<ApproveAssetAccountConstraints>,
    price_feed: Address,
) -> Result<()> {
    *context.accounts.approved_asset = ApprovedAsset {
        registry: *context.accounts.registry.address(),
        mint: *context.accounts.asset_mint.address(),
        price_feed,
        bump: context.bumps.approved_asset,
    };
    Ok(())
}
