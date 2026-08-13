use anchor_lang::prelude::*;

use crate::error::VaultError;
use crate::state::{AssetConfig, Strategy};

#[derive(Accounts)]
pub struct SetWeightAccountConstraints {
    pub manager: Signer,

    #[account(
        mut,
        has_one = manager,
        seeds = [b"strategy", strategy.index.to_le_bytes().as_ref()],
        bump = strategy.bump
    )]
    pub strategy: Box<BorshAccount<Strategy>>,

    #[account(
        mut,
        constraint = asset_config.strategy == strategy.address() @ VaultError::InvalidAssetAccount,
    )]
    pub asset_config: Box<BorshAccount<AssetConfig>>,
}

/// Change an asset's target weight. Setting it to zero retires the asset: deposits
/// stop allocating to it, and the manager sells its holdings out with `rebalance`,
/// leaving an empty vault at the asset's index. The index is never reused, so the
/// contiguous 0..asset_count range the valuation handlers depend on stays intact.
/// Funds do not move here; this only edits the target the manager trades toward.
pub fn handle_set_weight(
    context: &mut Context<SetWeightAccountConstraints>,
    weight_bps: u16,
) -> Result<()> {
    let strategy = &mut context.accounts.strategy;
    let asset_config = &mut context.accounts.asset_config;

    // total_weight_bps = total_weight_bps - old_weight + new_weight, kept <= 10000.
    let new_total = strategy
        .total_weight_bps
        .checked_sub(asset_config.weight_bps)
        .ok_or(VaultError::MathOverflow)?
        .checked_add(weight_bps)
        .ok_or(VaultError::MathOverflow)?;
    require!(new_total <= 10_000, VaultError::WeightOverflow);

    asset_config.weight_bps = weight_bps;
    strategy.total_weight_bps = new_total;

    Ok(())
}
