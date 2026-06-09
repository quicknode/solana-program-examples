use anchor_lang::prelude::*;

pub const MARKET_SEED: &[u8] = b"market";

// A Market is one trading pair (base/quote) with its own vaults and order book.
// The market PDA itself is the authority of the token vaults, so funds can only
// move out via program-signed CPIs (place/cancel/settle).
#[derive(InitSpace)]
#[account]
pub struct Market {
    pub authority: Pubkey,

    pub base_mint: Pubkey,

    pub quote_mint: Pubkey,

    pub base_vault: Pubkey,

    pub quote_vault: Pubkey,

    // Dedicated token account (quote mint) that accumulates taker fees.
    // Kept separate from `quote_vault` so user-owed balances and
    // market-earned fees cannot be confused. The market PDA signs transfers
    // out of it, so only program instruction handlers (notably `withdraw_fees`)
    // can drain it.
    pub fee_vault: Pubkey,

    pub order_book: Pubkey,

    pub fee_basis_points: u16,

    pub tick_size: u64,

    // Number of raw base-token units per lot. Quantities throughout the
    // program are in lots; this factor converts them to raw token units for
    // SPL transfers. For a base mint with d_base decimals and quote with
    // d_quote, set base_lot_size = 10^(d_base - d_quote) so that one raw
    // quote unit buys exactly one lot of base at price = 1, making `price`
    // equal to the human-readable USDC-per-token rate.
    //
    // Example — NVDAx (8 dec) / USDC (6 dec):
    //   base_lot_size = 10^(8-6) = 100 raw NVDAx per lot
    //   price = 130  →  $130.00 per NVDAx share
    //   tick_size = 1  →  $1.00 minimum price increment
    pub base_lot_size: u64,

    pub min_order_size: u64,

    pub is_active: bool,

    pub bump: u8,
}
