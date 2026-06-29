use anchor_lang::prelude::*;

/// Largest number of basket assets one strategy can hold. Not a storage limit
/// (each asset is its own account); the cap keeps deposit and withdraw, which
/// must reference every asset at once, within transaction account limits. USDC
/// is the base currency, held separately, and does not count against this.
pub const MAX_ASSETS: u8 = 8;

/// One strategy (basket). Its address is a PDA seeded by a caller-chosen index,
/// e.g. seeds `"strategy" + 0`, so strategies are addressed by a simple counter
/// rather than by the manager's key. The index is stored here so every handler
/// can re-derive the PDA to sign for the vaults and share mint.
#[account]
#[derive(InitSpace)]
pub struct Strategy {
    /// Index used as the PDA seed, e.g. 0 for the first strategy.
    pub index: u64,
    pub manager: Pubkey,
    /// Whitelist this strategy draws assets from. add_asset only accepts mints
    /// approved in this registry.
    pub registry: Pubkey,
    pub share_mint: Pubkey,
    pub usdc_mint: Pubkey,
    pub swap_router: Pubkey,
    /// Annual management fee in basis points (e.g. 100 = 1%).
    pub fee_bps: u16,
    /// Maximum tolerated deviation, in basis points, between a swap's output and
    /// the Pyth-implied amount on invest/rebalance. Bounded by MAX_SLIPPAGE_BPS.
    pub max_slippage_bps: u16,
    pub total_shares: u64,
    pub last_fee_accrual_timestamp: i64,
    /// Assets live at PDAs indexed 0..asset_count, so callers can re-derive the
    /// complete set and no asset can be silently omitted from a NAV calculation.
    pub asset_count: u8,
    /// Running sum of every asset's target weight, kept <= 10000.
    pub total_weight_bps: u16,
    pub bump: u8,
}

/// One basket asset. Its address is a PDA seeded by the strategy and the asset's
/// index, so the full set is the contiguous range 0..asset_count: any handler
/// computing net asset value re-derives every index and refuses to proceed if an
/// asset account is missing.
#[account]
#[derive(InitSpace)]
pub struct AssetConfig {
    pub strategy: Pubkey,
    pub index: u8,
    pub mint: Pubkey,
    /// Pyth PriceUpdateV2 account, copied from the registry whitelist entry at
    /// add time so the manager cannot substitute a feed they control.
    pub price_feed: Pubkey,
    /// Strategy-owned associated token account holding this asset.
    pub vault: Pubkey,
    /// Target share of the strategy's value in basis points. Advisory: the
    /// manager maintains it with invest/rebalance; no handler enforces it on deposit.
    pub weight_bps: u16,
    pub bump: u8,
}

impl AssetConfig {
    /// Deserialize an AssetConfig passed via remaining_accounts to an owned value,
    /// verifying it is owned by this program and has the right discriminator.
    /// Avoids the lifetime invariance of `Account::try_from` on borrowed infos.
    pub fn load_checked(account: &AccountInfo) -> Result<AssetConfig> {
        require_keys_eq!(
            *account.owner,
            crate::ID,
            crate::error::VaultError::InvalidAssetAccount
        );
        let data = account.try_borrow_data()?;
        AssetConfig::try_deserialize(&mut &data[..])
            .map_err(|_| error!(crate::error::VaultError::InvalidAssetAccount))
    }
}
