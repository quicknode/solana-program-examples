use quasar_lang::prelude::*;
use quasar_spl::prelude::*;

use crate::errors::FundError;
use crate::state::{
    snapshot_fund, ApprovedAsset, AssetConfig, AssetConfigInner, AssetVaultPda, Fund, Registry,
    MAX_ASSETS,
};

#[derive(Accounts)]
pub struct AddAssetAccountConstraints {
    #[account(mut)]
    pub manager: Signer,

    #[account(
        mut,
        address = Fund::seeds(fund.index.into()),
        has_one(manager),
        has_one(registry) @ FundError::InvalidRegistry,
    )]
    pub fund: Account<Fund>,

    pub registry: Account<Registry>,

    pub asset_mint: Account<Mint>,

    /// Proof the mint is approved and the source of its official price feed.
    #[account(address = ApprovedAsset::seeds(registry.address(), asset_mint.address()))]
    pub approved_asset: Account<ApprovedAsset>,

    #[account(
        init,
        payer = manager,
        address = AssetConfig::seeds(fund.address(), fund.asset_count),
    )]
    pub asset_config: Account<AssetConfig>,

    /// Fund-owned vault for this asset.
    #[account(
        init,
        payer = manager,
        token(mint = asset_mint, authority = fund, token_program = token_program),
        address = AssetVaultPda::seeds(fund.address(), fund.asset_count),
    )]
    pub vault_asset: Account<Token>,

    pub rent: Sysvar<Rent>,
    pub token_program: Program<TokenProgram>,
    pub system_program: Program<SystemProgram>,
}

#[inline(always)]
pub fn handle_add_asset(
    accounts: &mut AddAssetAccountConstraints,
    weight_bps: u16,
    bumps: &AddAssetAccountConstraintsBumps,
) -> Result<(), ProgramError> {
    require!(
        accounts.fund.asset_count < MAX_ASSETS,
        FundError::TooManyAssets
    );

    let total_weight = u16::from(accounts.fund.total_weight_bps);
    let new_total = (total_weight as u32)
        .checked_add(weight_bps as u32)
        .ok_or(FundError::MathOverflow)?;
    require!(new_total <= 10_000, FundError::WeightOverflow);

    let index = accounts.fund.asset_count;

    accounts.asset_config.set_inner(AssetConfigInner {
        fund: *accounts.fund.address(),
        index,
        mint: *accounts.asset_mint.address(),
        decimals: accounts.asset_mint.decimals,
        // Copied from the registry entry, never supplied by the manager.
        price_feed: accounts.approved_asset.price_feed,
        vault: *accounts.vault_asset.address(),
        weight_bps,
        bump: bumps.asset_config,
    });

    let mut fund = snapshot_fund(&accounts.fund);
    fund.asset_count = index.checked_add(1).ok_or(FundError::MathOverflow)?;
    fund.total_weight_bps = new_total as u16;
    accounts.fund.set_inner(fund);
    Ok(())
}
