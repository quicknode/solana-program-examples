use anchor_lang::prelude::*;

use crate::error::FundError;
use crate::state::{AssetConfig, Fund};

#[derive(Accounts)]
pub struct SetWeightAccountConstraints<'info> {
    pub manager: Signer<'info>,

    #[account(
        mut,
        has_one = manager,
        seeds = [b"fund", fund.index.to_le_bytes().as_ref()],
        bump = fund.bump
    )]
    pub fund: Box<Account<'info, Fund>>,

    #[account(
        mut,
        constraint = asset_config.fund == fund.key() @ FundError::InvalidAssetAccount,
    )]
    pub asset_config: Box<Account<'info, AssetConfig>>,
}

/// Change an asset's target weight. Setting it to zero retires the asset: deposits
/// stop allocating to it, and the manager sells its holdings out with `rebalance`,
/// leaving an empty vault at the asset's index. The index is never reused, so the
/// contiguous 0..asset_count range the valuation handlers depend on stays intact.
/// Funds do not move here; this only edits the target the manager trades toward.
pub fn handle_set_weight(
    context: Context<SetWeightAccountConstraints>,
    weight_bps: u16,
) -> Result<()> {
    let fund = &mut context.accounts.fund;
    let asset_config = &mut context.accounts.asset_config;

    // total_weight_bps = total_weight_bps - old_weight + new_weight, kept <= 10000.
    let new_total = fund
        .total_weight_bps
        .checked_sub(asset_config.weight_bps)
        .ok_or(FundError::MathOverflow)?
        .checked_add(weight_bps)
        .ok_or(FundError::MathOverflow)?;
    require!(new_total <= 10_000, FundError::WeightOverflow);

    asset_config.weight_bps = weight_bps;
    fund.total_weight_bps = new_total;

    Ok(())
}
