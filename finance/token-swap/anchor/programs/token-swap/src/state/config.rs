use anchor_lang::prelude::*;

/// Shared configuration for the AMM (admin + trading fee).
///
/// `Config` is a singleton: one account per deployed program, seeded by the
/// fixed byte string `b"config"`. This mirrors how real DEXs are deployed in
/// practice (e.g. Phoenix and Raydium ship one program per market/AMM, so the
/// program-level config is global by construction). Parameterising the config
/// by an `id` was leftover complexity from the original example; removing it
/// makes the onchain layout simpler and matches realistic deployment.
#[account(borsh)]
#[derive(Default, InitSpace)]
pub struct Config {
    /// Account that has admin authority over the AMM.
    pub admin: Address,

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
    /// every fee, LPs keep the rest. Set in `initialize_config`; fixed for the
    /// lifetime of the program. Must be `< 10_000`.
    pub admin_share_bps: u16,

    /// Canonical bump for this PDA.
    pub bump: u8,
}
