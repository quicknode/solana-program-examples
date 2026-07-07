use quasar_lang::prelude::*;

use crate::errors::VaultError;

pub const STRATEGY_SEED: &[u8] = b"strategy";
pub const ASSET_CONFIG_SEED: &[u8] = b"asset";

/// Largest number of basket assets one strategy can hold. Each asset is its own
/// account, so this is not a storage limit; it bounds how many accounts a
/// deposit or withdraw (which reference every asset at once) must pass in a
/// single transaction.
pub const MAX_ASSETS: u8 = 16;

/// One strategy (basket). PDA `["strategy", index]`, addressed by a
/// caller-chosen counter rather than the manager's key. The index is stored so
/// every handler can re-derive the PDA to sign for the vaults and share mint.
#[account(discriminator = 3, set_inner)]
#[seeds(b"strategy", index: u64)]
pub struct Strategy {
    pub index: u64,
    pub manager: Address,
    pub registry: Address,
    pub share_mint: Address,
    pub usdc_mint: Address,
    pub swap_router: Address,
    pub fee_bps: u16,
    pub max_slippage_bps: u16,
    pub total_shares: u64,
    pub last_fee_accrual_timestamp: i64,
    pub asset_count: u8,
    pub total_weight_bps: u16,
    pub bump: u8,
}

/// One basket asset. PDA `["asset", strategy, index]`, so the full set is the
/// contiguous range `0..asset_count`: any handler computing net asset value
/// re-derives every index and refuses to proceed if an asset account is missing.
#[account(discriminator = 4, set_inner)]
#[seeds(b"asset", strategy: Address, index: u8)]
pub struct AssetConfig {
    pub strategy: Address,
    pub index: u8,
    pub mint: Address,
    /// Price feed account, copied from the registry whitelist entry at add time
    /// so the manager cannot substitute a feed they control.
    pub price_feed: Address,
    /// Strategy-owned token account holding this asset.
    pub vault: Address,
    pub weight_bps: u16,
    pub bump: u8,
}

/// PDA marker for a strategy's share mint: `["share_mint", strategy]`.
#[derive(Seeds)]
#[seeds(b"share_mint", strategy: Address)]
pub struct ShareMintPda;

/// PDA marker for a strategy's USDC vault: `["usdc_vault", strategy]`.
#[derive(Seeds)]
#[seeds(b"usdc_vault", strategy: Address)]
pub struct UsdcVaultPda;

/// PDA marker for one asset's vault: `["asset_vault", strategy, index]`.
#[derive(Seeds)]
#[seeds(b"asset_vault", strategy: Address, index: u8)]
pub struct AssetVaultPda;

pub fn snapshot_strategy(strategy: &Account<Strategy>) -> StrategyInner {
    StrategyInner {
        index: u64::from(strategy.index),
        manager: strategy.manager,
        registry: strategy.registry,
        share_mint: strategy.share_mint,
        usdc_mint: strategy.usdc_mint,
        swap_router: strategy.swap_router,
        fee_bps: u16::from(strategy.fee_bps),
        max_slippage_bps: u16::from(strategy.max_slippage_bps),
        total_shares: u64::from(strategy.total_shares),
        last_fee_accrual_timestamp: i64::from(strategy.last_fee_accrual_timestamp),
        asset_count: strategy.asset_count,
        total_weight_bps: u16::from(strategy.total_weight_bps),
        bump: strategy.bump,
    }
}

/// Read-only view of one asset config's fields, used both for declared accounts
/// and for configs passed via remaining accounts.
pub struct AssetConfigView {
    pub strategy: Address,
    pub index: u8,
    pub mint: Address,
    pub price_feed: Address,
    pub vault: Address,
    pub weight_bps: u16,
}

/// Validate a remaining-account AssetConfig (owner + discriminator) and copy its
/// fields out. Mirrors the Anchor build's `AssetConfig::load_checked`.
pub fn load_asset_config(view: &AccountView) -> Result<AssetConfigView, ProgramError> {
    let account = Account::<AssetConfig>::from_account_view(view)
        .map_err(|_| ProgramError::from(VaultError::InvalidAssetAccount))?;
    Ok(AssetConfigView {
        strategy: account.strategy,
        index: account.index,
        mint: account.mint,
        price_feed: account.price_feed,
        vault: account.vault,
        weight_bps: u16::from(account.weight_bps),
    })
}
