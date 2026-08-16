use anchor_lang::prelude::*;

/// Largest number of basket assets one strategy can hold. Not a storage limit
/// (each asset is its own account); the cap bounds how many accounts deposit and
/// withdraw, which must reference every asset at once, pull into a single
/// instruction: deposit uses 14 + 5*N accounts and withdraw 10 + 4*N, where N is the
/// asset count. At the cap of 16 that is 94 accounts for deposit (74 for withdraw),
/// within Solana's 128-account transaction lock limit but past the 1232-byte legacy
/// transaction size (which fits only ~3 assets), so a client depositing into a large
/// basket must send a v0 transaction with an Address Lookup Table. USDC is the base
/// currency, held separately, and does not count against this.
pub const MAX_ASSETS: u8 = 16;

/// One strategy (basket). Its address is a PDA seeded by a caller-chosen index,
/// e.g. seeds `"strategy" + 0`, so strategies are addressed by a simple counter
/// rather than by the manager's key. The index is stored here so every handler
/// can re-derive the PDA to sign for the vaults and share mint.
#[account(borsh)]
#[derive(InitSpace)]
pub struct Strategy {
    /// Index used as the PDA seed, e.g. 0 for the first strategy.
    pub index: u64,
    pub manager: Address,
    /// Registry whose curator approves assets. add_asset only accepts mints
    /// approved in this registry.
    pub registry: Address,
    pub share_mint: Address,
    pub usdc_mint: Address,
    pub swap_router: Address,
    /// Annual management fee in basis points (e.g. 100 = 1%).
    pub fee_bps: u16,
    /// Maximum tolerated deviation, in basis points, between a swap's output and
    /// the Pyth-implied amount on deposit/rebalance. Bounded by MAX_SLIPPAGE_BPS.
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
#[account(borsh)]
#[derive(InitSpace)]
pub struct AssetConfig {
    pub strategy: Address,
    pub index: u8,
    pub mint: Address,
    /// Pyth PriceUpdateV2 account, copied from the registry's ApprovedAsset at
    /// add time so the manager cannot substitute a feed they control.
    pub price_feed: Address,
    /// Strategy-owned associated token account holding this asset.
    pub vault: Address,
    /// Target share of the strategy's value in basis points. deposit deploys at these
    /// weights (the sum across assets must reach 10000 before deposits open), and the
    /// manager maintains them against price drift with rebalance.
    pub weight_bps: u16,
    pub bump: u8,
}

impl AssetConfig {
    /// Deserialize an AssetConfig passed via remaining_accounts to an owned value,
    /// verifying it is owned by this program and has the right discriminator.
    /// Avoids the lifetime invariance of `Account::try_from` on borrowed infos.
    pub fn load_checked(account: &AccountView) -> Result<AssetConfig> {
        require_keys_eq!(
            *account.owner(),
            crate::ID,
            crate::error::VaultError::InvalidAssetAccount
        );
        let data = account.try_borrow()?;
        let disc_len = <AssetConfig as anchor_lang::Discriminator>::DISCRIMINATOR.len();
        require!(
            data.len() > disc_len
                && &data[..disc_len] == <AssetConfig as anchor_lang::Discriminator>::DISCRIMINATOR,
            crate::error::VaultError::InvalidAssetAccount
        );
        let mut payload = &data[disc_len..];
        <AssetConfig as wincode::SchemaRead<anchor_lang::BorshConfig>>::get(&mut payload)
            .map_err(|_| crate::error::VaultError::InvalidAssetAccount.into())
    }
}
