use quasar_lang::prelude::*;
use quasar_spl::prelude::*;

use crate::errors::VaultError;
use crate::state::{
    snapshot_strategy, ApprovedAsset, AssetConfig, AssetConfigInner, AssetVaultPda, Registry,
    Strategy, MAX_ASSETS,
};

#[derive(Accounts)]
pub struct AddAssetAccountConstraints {
    #[account(mut)]
    pub manager: Signer,

    #[account(
        mut,
        address = Strategy::seeds(strategy.index.into()),
        has_one(manager),
        has_one(registry) @ VaultError::InvalidRegistry,
    )]
    pub strategy: Account<Strategy>,

    pub registry: Account<Registry>,

    pub asset_mint: Account<Mint>,

    /// Proof the mint is approved and the source of its official price feed.
    #[account(address = ApprovedAsset::seeds(registry.address(), asset_mint.address()))]
    pub approved_asset: Account<ApprovedAsset>,

    #[account(
        init,
        payer = manager,
        address = AssetConfig::seeds(strategy.address(), strategy.asset_count),
    )]
    pub asset_config: Account<AssetConfig>,

    /// Strategy-owned vault for this asset.
    #[account(
        init,
        payer = manager,
        token(mint = asset_mint, authority = strategy, token_program = token_program),
        address = AssetVaultPda::seeds(strategy.address(), strategy.asset_count),
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
        accounts.strategy.asset_count < MAX_ASSETS,
        VaultError::TooManyAssets
    );

    let total_weight = u16::from(accounts.strategy.total_weight_bps);
    let new_total = (total_weight as u32)
        .checked_add(weight_bps as u32)
        .ok_or(VaultError::MathOverflow)?;
    require!(new_total <= 10_000, VaultError::WeightOverflow);

    let index = accounts.strategy.asset_count;

    accounts.asset_config.set_inner(AssetConfigInner {
        strategy: *accounts.strategy.address(),
        index,
        mint: *accounts.asset_mint.address(),
        // Copied from the registry entry, never supplied by the manager.
        price_feed: accounts.approved_asset.price_feed,
        vault: *accounts.vault_asset.address(),
        weight_bps,
        bump: bumps.asset_config,
    });

    let mut strategy = snapshot_strategy(&accounts.strategy);
    strategy.asset_count = index.checked_add(1).ok_or(VaultError::MathOverflow)?;
    strategy.total_weight_bps = new_total as u16;
    accounts.strategy.set_inner(strategy);
    Ok(())
}
