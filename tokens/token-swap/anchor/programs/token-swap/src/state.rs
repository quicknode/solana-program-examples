use anchor_lang::prelude::*;

/// Shared configuration for the AMM (admin + trading fee).
///
/// `Config` is a singleton: one account per deployed program, seeded by the
/// fixed byte string `b"config"`. This mirrors how real DEXs are deployed in
/// practice (e.g. Phoenix and Raydium ship one program per market/AMM, so the
/// program-level config is global by construction). Parameterising the config
/// by an `id` was leftover complexity from the original example; removing it
/// makes the on-chain layout simpler and matches realistic deployment.
#[account]
#[derive(Default, InitSpace)]
pub struct Config {
    /// Account that has admin authority over the AMM.
    pub admin: Pubkey,

    /// The trading fee taken on each swap, in basis points (out of 10_000).
    ///
    /// This is the *total* fee charged on a swap. It is split between LPs and
    /// the admin according to `admin_share_bps`.
    pub fee: u16,

    /// Fraction of the trading fee that goes to the admin, in basis points
    /// (out of 10_000). The remainder goes to LPs (it stays in the pool
    /// reserves and grows the LP-claimable balance).
    ///
    /// Modelled on Uniswap V2 / Raydium: the AMM operator takes a slice of
    /// every fee, LPs keep the rest. Set in `create_config`; fixed for the
    /// lifetime of the program. Must be `< 10_000`.
    pub admin_share_bps: u16,

    /// Canonical bump for this PDA.
    pub bump: u8,
}

/// Per-pool configuration / identity record.
///
/// Holds the metadata that identifies a single pool: which `Config` it belongs
/// to, which two mints it trades, and its canonical bump. The actual pool
/// reserves live in separate token accounts (`pool_a`, `pool_b`) owned by the
/// pool authority PDA — they are not stored here. This struct is the pool's
/// *configuration*, not its state.
///
/// In addition to the identity fields, this account tracks the admin's
/// accumulated trading-fee claim on each side (`admin_fees_owed_a` /
/// `admin_fees_owed_b`). Those fees physically sit in the existing `pool_a` /
/// `pool_b` reserves; the accumulators are a *virtual* obligation against
/// those balances. LP-facing math (deposit, withdraw, swap curve) uses
/// `pool_X.amount - admin_fees_owed_X` so the admin's owed slice is not
/// counted toward LP yield.
#[account]
#[derive(Default, InitSpace)]
pub struct PoolConfig {
    /// Address of the parent `Config` account this pool belongs to.
    pub config: Pubkey,

    /// Mint of token A.
    pub mint_a: Pubkey,

    /// Mint of token B.
    pub mint_b: Pubkey,

    /// Admin's accumulated fee claim on token A, in base units. Sits
    /// physically in `pool_a` but is excluded from the LP curve and from
    /// LP-withdrawable amounts. Swept by `claim_admin_fees`.
    pub admin_fees_owed_a: u64,

    /// Admin's accumulated fee claim on token B, in base units. Sits
    /// physically in `pool_b` but is excluded from the LP curve and from
    /// LP-withdrawable amounts. Swept by `claim_admin_fees`.
    pub admin_fees_owed_b: u64,

    /// Canonical bump for this PDA.
    pub bump: u8,
}
