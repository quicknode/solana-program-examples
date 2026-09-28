use quasar_lang::prelude::*;

use crate::errors::FundError;
use crate::state::{snapshot_fund, AssetConfig, AssetConfigInner, Fund};

#[derive(Accounts)]
pub struct SetWeightAccountConstraints {
    pub manager: Signer,

    #[account(mut, address = Fund::seeds(fund.index.into()), has_one(manager))]
    pub fund: Account<Fund>,

    #[account(
        mut,
        address = AssetConfig::seeds(fund.address(), asset_config.index),
    )]
    pub asset_config: Account<AssetConfig>,
}

/// Change an asset's target weight. Setting it to zero retires the asset:
/// deposits stop allocating to it and the manager sells its holdings out with
/// `rebalance`, leaving an empty vault at the asset's index. The index is never
/// reused, so the contiguous `0..asset_count` range the valuation handlers
/// depend on stays intact. Funds do not move here.
#[inline(always)]
pub fn handle_set_weight(
    accounts: &mut SetWeightAccountConstraints,
    weight_bps: u16,
) -> Result<(), ProgramError> {
    let total_weight = u16::from(accounts.fund.total_weight_bps);
    let old_weight = u16::from(accounts.asset_config.weight_bps);

    let new_total = total_weight
        .checked_sub(old_weight)
        .ok_or(FundError::MathOverflow)?
        .checked_add(weight_bps)
        .ok_or(FundError::MathOverflow)?;
    require!(new_total <= 10_000, FundError::WeightOverflow);

    let mut asset_config = AssetConfigInner {
        fund: accounts.asset_config.fund,
        index: accounts.asset_config.index,
        mint: accounts.asset_config.mint,
        price_feed: accounts.asset_config.price_feed,
        vault: accounts.asset_config.vault,
        weight_bps: old_weight,
        bump: accounts.asset_config.bump,
    };
    asset_config.weight_bps = weight_bps;
    accounts.asset_config.set_inner(asset_config);

    let mut fund = snapshot_fund(&accounts.fund);
    fund.total_weight_bps = new_total;
    accounts.fund.set_inner(fund);
    Ok(())
}
