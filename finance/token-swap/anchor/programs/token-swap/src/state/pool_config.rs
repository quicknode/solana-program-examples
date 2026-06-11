use anchor_lang::prelude::*;

/// Per-pool configuration / identity record.
///
/// Holds the metadata that identifies a single pool: which `Config` it belongs
/// to, which two mints it trades, and its canonical bump. The actual pool
/// reserves live in separate token accounts (`pool_a`, `pool_b`) owned by the
/// pool authority PDA - they are not stored here. This struct is the pool's
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
