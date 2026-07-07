use quasar_lang::prelude::*;

use crate::errors::VaultError;
use crate::state::{snapshot_strategy, AssetConfig, AssetConfigInner, Strategy};

#[derive(Accounts)]
pub struct SetWeightAccountConstraints {
    pub manager: Signer,

    #[account(mut, address = Strategy::seeds(strategy.index.into()), has_one(manager))]
    pub strategy: Account<Strategy>,

    #[account(
        mut,
        address = AssetConfig::seeds(strategy.address(), asset_config.index),
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
    let total_weight = u16::from(accounts.strategy.total_weight_bps);
    let old_weight = u16::from(accounts.asset_config.weight_bps);

    let new_total = total_weight
        .checked_sub(old_weight)
        .ok_or(VaultError::MathOverflow)?
        .checked_add(weight_bps)
        .ok_or(VaultError::MathOverflow)?;
    require!(new_total <= 10_000, VaultError::WeightOverflow);

    let mut asset_config = AssetConfigInner {
        strategy: accounts.asset_config.strategy,
        index: accounts.asset_config.index,
        mint: accounts.asset_config.mint,
        price_feed: accounts.asset_config.price_feed,
        vault: accounts.asset_config.vault,
        weight_bps: old_weight,
        bump: accounts.asset_config.bump,
    };
    asset_config.weight_bps = weight_bps;
    accounts.asset_config.set_inner(asset_config);

    let mut strategy = snapshot_strategy(&accounts.strategy);
    strategy.total_weight_bps = new_total;
    accounts.strategy.set_inner(strategy);
    Ok(())
}
