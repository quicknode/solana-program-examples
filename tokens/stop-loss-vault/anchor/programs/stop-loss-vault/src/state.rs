use anchor_lang::prelude::*;

/// Per-owner stop-loss vault.
///
/// One vault holds ONE volatile asset and converts it into ONE stable asset
/// when a Switchboard On-Demand price feed reports a price below
/// `threshold_price`. The vault is permissionlessly cranked: anyone can call
/// `convert_if_triggered`, but the call only succeeds when the price has
/// crossed the threshold. TukTuk is used in production to schedule that crank
/// every `crank_interval_seconds`.
#[derive(InitSpace)]
#[account]
pub struct Vault {
    /// Wallet that initialised the vault. Only this key can deposit,
    /// withdraw, update the threshold, or close the vault.
    pub owner: Pubkey,

    /// Mint of the volatile asset (e.g. wSOL).
    pub volatile_mint: Pubkey,

    /// Mint of the stable asset that the vault converts into (e.g. USDC).
    pub stable_mint: Pubkey,

    /// Switchboard On-Demand feed reporting `volatile_mint / USD`. Layout is
    /// validated at read time.
    pub oracle_feed: Pubkey,

    /// Threshold price expressed in the feed's native scale (e.g. for an
    /// 8-decimal feed, `threshold_price = 100_00000000` means $100). When the
    /// feed reports `<= threshold_price` the cranker can fire the swap.
    /// Stored as `i128` to match Switchboard's price type.
    pub threshold_price: i128,

    /// Suggested crank cadence in seconds. The on-chain program does NOT
    /// enforce this — it's a hint to the offchain cranker (TukTuk) on how
    /// often to attempt the conversion. Default in `initialize_vault` is 600.
    pub crank_interval_seconds: u32,

    /// Public key of the TukTuk task registered against this vault. Stored so
    /// the owner can find / cancel / reconfigure the task offchain.
    pub tuktuk_task: Pubkey,

    /// Set to `true` after `convert_if_triggered` fires. Locks `deposit` and
    /// the threshold update path so the vault can't be re-armed without
    /// closing — keeping the example simple.
    pub triggered: bool,

    /// PDA bump for `[b"vault", owner.key().as_ref()]`.
    pub bump: u8,
}

impl Vault {
    pub const SEED_PREFIX: &'static [u8] = b"vault";
}
