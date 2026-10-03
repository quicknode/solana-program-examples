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
    pub underlying_owed: u64,
    /// Quote minor units the vault owes: put writers' collateral, plus call
    /// holders' strike payments awaiting the writer's `collect_proceeds`.
    pub quote_owed: u64,
    /// Quote minor units held for the admin, swept by `collect_fees`.
    pub fees_owed: u64,
    /// Fee charged on each premium, in basis points.
    pub fee_bps: u16,
    /// Bump of this account's own PDA. The market is the token authority of
    /// both vaults and signs every transfer out of them with its seeds.
    pub bump: u8,
}

/// One option. Mirrors the Anchor `OptionContract`; `kind` and `status`
/// are `u8` (see `constants.rs`) because the account layout is zero-copy.
///
/// The option stores the two amounts that change hands, so settlement does no
/// arithmetic.
#[account(discriminator = 2, set_inner)]
#[seeds(b"option", market: Address, writer: Address, id: u64)]
pub struct OptionContract {
    pub id: u64,
    pub market: Address,
    pub writer: Address,
    /// The buyer, once there is one. All zeroes while listed.
    pub holder: Address,
    /// Underlying minor units the option covers: what a call writer posts and
    /// a call holder receives, or a put holder delivers.
    pub underlying_amount: u64,
    /// Quote minor units paid for the underlying on exercise: what a put
    /// writer posts and a put holder receives, or a call holder pays. The
    /// strike for the whole option, as an amount rather than a price.
    pub strike_amount: u64,
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

/// Underlying-token vault PDA at seeds = [b"underlying_vault", market].
#[derive(Seeds)]
#[seeds(b"underlying_vault", market: Address)]
pub struct UnderlyingVaultPda;

/// Quote-token vault PDA at seeds = [b"quote_vault", market].
#[derive(Seeds)]
#[seeds(b"quote_vault", market: Address)]
pub struct QuoteVaultPda;
