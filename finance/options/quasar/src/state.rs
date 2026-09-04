use quasar_lang::prelude::*;

/// One options venue. Mirrors the Anchor `Market` field-for-field; see the
/// Anchor sibling's README for what each field means. The three `*_locked` /
/// `fees_owed` counters are the ledger of what each vault owes, asserted
/// against the vault balances after every transfer.
#[account(discriminator = 1, set_inner)]
#[seeds(b"market", underlying_mint: Address, quote_mint: Address)]
pub struct Market {
    pub admin: Address,
    pub underlying_mint: Address,
    pub quote_mint: Address,
    pub underlying_vault: Address,
    pub quote_vault: Address,
    /// Underlying minor units the vault owes: call writers' collateral, plus
    /// put holders' deliveries awaiting the writer's `collect_proceeds`.
    pub underlying_locked: u64,
    /// Quote minor units the vault owes: put writers' collateral, plus call
    /// holders' strike payments awaiting the writer's `collect_proceeds`.
    pub quote_locked: u64,
    /// Quote minor units held for the admin, swept by `collect_fees`.
    pub fees_owed: u64,
    /// Fee charged on each premium, in basis points.
    pub fee_bps: u16,
    pub bump: u8,
    pub authority_bump: u8,
}

/// One option lot. Mirrors the Anchor `OptionContract`; `kind` and `status`
/// are `u8` (see `constants.rs`) because the account layout is zero-copy.
///
/// Every amount the lot ever moves is a product of two of its integers:
/// `contracts * underlying_per_contract` of the underlying, and
/// `contracts * strike_per_contract` of the quote token.
#[account(discriminator = 2, set_inner)]
#[seeds(b"option", market: Address, writer: Address, id: u64)]
pub struct OptionContract {
    pub id: u64,
    pub market: Address,
    pub writer: Address,
    /// The buyer, once there is one. All zeroes while listed.
    pub holder: Address,
    pub contracts: u64,
    pub underlying_per_contract: u64,
    pub strike_per_contract: u64,
    pub premium: u64,
    /// Unix timestamp after which the holder can no longer exercise and the
    /// writer may reclaim the collateral. Wall-clock time because an option's
    /// expiry is a calendar date the parties agreed on; the program reads no
    /// oracle, so slot-measured freshness never enters into it.
    pub expiry: i64,
    pub kind: u8,
    pub status: u8,
    pub bump: u8,
}

/// Authority PDA at seeds = [b"authority", market]. Holds no data; signs
/// every transfer out of either vault.
#[derive(Seeds)]
#[seeds(b"authority", market: Address)]
pub struct MarketAuthorityPda;

/// Underlying-token vault PDA at seeds = [b"underlying_vault", market].
#[derive(Seeds)]
#[seeds(b"underlying_vault", market: Address)]
pub struct UnderlyingVaultPda;

/// Quote-token vault PDA at seeds = [b"quote_vault", market].
#[derive(Seeds)]
#[seeds(b"quote_vault", market: Address)]
pub struct QuoteVaultPda;
