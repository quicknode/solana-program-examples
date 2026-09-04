use anchor_lang::prelude::*;

/// One options venue: every option written here is on the same underlying
/// token, exercised in the same quote token, and collateralized in one of the
/// two vaults.
///
/// The vaults hold other people's money (writers' collateral, and the strike
/// payments holders make at exercise), so the two `*_locked` fields say how
/// much of each vault the market owes and to whom it is owed in aggregate.
/// Every handler that moves tokens asserts, after its own arithmetic, that
/// each vault still covers what the market owes.
#[account(borsh)]
#[derive(InitSpace)]
pub struct Market {
    /// Operates the venue and collects the fee on every premium. Cannot touch
    /// collateral or strike payments: no handler moves either to the admin.
    pub admin: Address,

    /// The asset the options are written on (NVDAx in the walkthrough).
    pub underlying_mint: Address,

    /// The asset premiums are paid in and strikes are settled in (USDC).
    pub quote_mint: Address,

    pub underlying_vault: Address,

    pub quote_vault: Address,

    /// Underlying minor units the vault owes: call writers' collateral, plus
    /// put holders' deliveries awaiting the writer's `collect_proceeds`.
    pub underlying_locked: u64,

    /// Quote minor units the vault owes: put writers' collateral, plus call
    /// holders' strike payments awaiting the writer's `collect_proceeds`.
    pub quote_locked: u64,

    /// Quote minor units held in the quote vault for the admin, accrued from
    /// the fee on each premium and swept by `collect_fees`.
    pub fees_owed: u64,

    /// Fee charged on each premium, in basis points. The venue's revenue.
    pub fee_bps: u16,

    pub bump: u8,

    /// Bump for the vault authority PDA, stored so CPIs can sign without
    /// re-deriving it.
    pub authority_bump: u8,
}
