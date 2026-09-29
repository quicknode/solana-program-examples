use quasar_lang::prelude::*;
use quasar_spl::prelude::*;

use crate::state::{ApprovedAsset, ApprovedAssetInner, Registry};

#[derive(Accounts)]
pub struct ApproveAssetAccountConstraints {
    #[account(mut)]
    pub authority: Signer,

    #[account(address = Registry::seeds(authority.address()), has_one(authority))]
    pub registry: Account<Registry>,

    pub asset_mint: Account<Mint>,

    #[account(
        init,
        payer = authority,
        address = ApprovedAsset::seeds(registry.address(), asset_mint.address()),
    )]
    pub approved_asset: Account<ApprovedAsset>,

    pub rent: Sysvar<Rent>,
    pub system_program: Program<SystemProgram>,
}

#[inline(always)]
pub fn handle_approve_asset(
    accounts: &mut ApproveAssetAccountConstraints,
    price_feed: Address,
    bumps: &ApproveAssetAccountConstraintsBumps,
) -> Result<(), ProgramError> {
    accounts.approved_asset.set_inner(ApprovedAssetInner {
        registry: *accounts.registry.address(),
        mint: *accounts.asset_mint.address(),
        price_feed,
        bump: bumps.approved_asset,
    });
    Ok(())
}
