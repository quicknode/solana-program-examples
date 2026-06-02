use quasar_lang::prelude::*;

/// Shared configuration for the AMM (admin + trading fee).
///
/// `Config` is a singleton: one account per deployed program, seeded by the
/// fixed byte string `b"config"`. This mirrors how real DEXs are deployed in
/// practice (e.g. Phoenix and Raydium ship one program per market/AMM, so the
/// program-level config is global by construction). Every `PoolConfig`
/// references this `Config` for fee/admin parameters.
#[account(discriminator = 100, set_inner)]
pub struct Config {
    /// Admin authority.
    pub admin: Address,
    /// Total trading fee in basis points (e.g. 30 = 0.3%). Split between LPs
    /// and the admin according to `admin_share_bps`.
    pub fee: u16,
    /// Fraction of the trading fee that goes to the admin, in basis points
    /// (out of 10_000). The remainder goes to LPs (it stays in the pool
    /// reserves and grows the LP-claimable balance). Must be < 10_000.
    pub admin_share_bps: u16,
}

/// Per-pool configuration / identity record linking an AMM `Config` to a pair
/// of token mints.
///
/// Holds the metadata that identifies a single pool: which `Config` it belongs
/// to and which two mints it trades. The actual pool reserves live in separate
/// token accounts (`pool_a`, `pool_b`) owned by the pool authority PDA — they
/// are not stored here. This struct is the pool's *configuration*, not its
/// state.
///
/// Also tracks the admin's accumulated trading-fee claim per side
/// (`admin_fees_owed_a` / `admin_fees_owed_b`). Those amounts physically sit
/// in the existing `pool_a` / `pool_b` reserves; the accumulators are a
/// *virtual* obligation against those balances. LP-facing math (deposit,
/// withdraw, swap curve) uses `pool_X.amount() - admin_fees_owed_X` so the
/// admin's owed slice is not counted toward LP yield.
#[account(discriminator = 101, set_inner)]
pub struct PoolConfig {
    /// Address of the parent `Config` account this pool belongs to.
    pub config: Address,
    /// Mint of token A.
    pub mint_a: Address,
    /// Mint of token B.
    pub mint_b: Address,
    /// Admin's accumulated fee claim on token A, in base units. Sits
    /// physically in `pool_a` but excluded from the LP curve and from
    /// LP-withdrawable amounts. Swept by `claim_admin_fees`.
    pub admin_fees_owed_a: u64,
    /// Admin's accumulated fee claim on token B, in base units. Sits
    /// physically in `pool_b` but excluded from the LP curve and from
    /// LP-withdrawable amounts. Swept by `claim_admin_fees`.
    pub admin_fees_owed_b: u64,
}
