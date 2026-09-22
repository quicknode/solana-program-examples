use quasar_lang::prelude::*;

pub const MARKET_SEED: &[u8] = b"market";

/// The three vaults are PDAs of the market: each is found from its own seed
/// and the market's address, so any client can derive where the market keeps
/// its tokens without reading the market first. The market also records each
/// address, and every handler that touches a vault checks it against that
/// record, which is what stops the fee vault being passed as the quote vault.
///
/// Base-token vault PDA at seeds = [b"base_vault", market].
#[derive(Seeds)]
#[seeds(b"base_vault", market: Address)]
pub struct BaseVaultPda;

/// Quote-token vault PDA at seeds = [b"quote_vault", market].
#[derive(Seeds)]
#[seeds(b"quote_vault", market: Address)]
pub struct QuoteVaultPda;

/// Fee vault PDA (quote mint) at seeds = [b"fee_vault", market].
#[derive(Seeds)]
#[seeds(b"fee_vault", market: Address)]
pub struct FeeVaultPda;

/// A Market is one trading pair (base/quote) with its own vaults and order
/// book. The market PDA itself is the authority of the token vaults, so funds
/// can only move out via program-signed CPIs (place/cancel/settle).
///
/// PDA: `["market", base_mint, quote_mint]`.
#[account(discriminator = 1, set_inner)]
#[seeds(b"market", base_mint: Address, quote_mint: Address)]
pub struct Market {
    pub authority: Address,
    pub base_mint: Address,
    pub quote_mint: Address,
    pub base_vault: Address,
    pub quote_vault: Address,

    /// Dedicated token account (quote mint) that accumulates taker fees. Kept
    /// separate from `quote_vault` so user-owed balances and market-earned
    /// fees cannot be confused. The market PDA signs transfers out of it, so
    /// only program instruction handlers (notably `withdraw_fees`) can drain
    /// it.
    pub fee_vault: Address,

    /// The order-book account (created directly by the client, not a PDA - see
    /// `initialize_market`). Bound to this market via this stored address.
    pub order_book: Address,

    pub fee_basis_points: u16,
    pub tick_size: u64,

    // Two-lot model (mirrors Serum/Openbook): both sides of the book are
    // denominated in their respective lots rather than raw token units.
    //
    //   raw_base  = quantity × base_lot_size
    //   raw_quote = quantity × price × quote_lot_size
    //
    // Choose:
    //   base_lot_size  = 10^max(d_base  − d_quote, 0)
    //   quote_lot_size = 10^max(d_quote − d_base,  0)
    //
    // so `price` reads as the human-readable quote/base rate and
    // `tick_size = 1` is a single atomic increment.
    pub base_lot_size: u64,
    pub quote_lot_size: u64,
    pub min_order_size: u64,
    pub is_active: PodBool,
    pub bump: u8,
}
