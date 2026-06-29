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

    // Two-lot model (mirrors Serum/Openbook): both sides of the book are
    // denominated in their respective lots rather than raw token units.
    // This makes `price` and `quantity` human-readable regardless of the
    // individual mints' decimal counts.
    //
    //   raw_base  = quantity  × base_lot_size
    //   raw_quote = quantity  × price × quote_lot_size
    //
    // Choose:
    //   base_lot_size  = 10^max(d_base  − d_quote, 0)
    //   quote_lot_size = 10^max(d_quote − d_base,  0)
    //
    // so that exactly one of the two is > 1 (or both are 1 when d_base == d_quote).
    // With those values `price` equals the human-readable quote/base rate and
    // `tick_size = 1` is a single atomic increment.
    //
    // Examples:
    //   NVDAx (8 dec) / USDC (6 dec):  base_lot_size=100, quote_lot_size=1
    //     price=130, qty=1 lot  → 130 × 1 × 1 = 130 raw USDC per 100 raw NVDAx
    //     = $130.00 per NVDAx share ✓
    //
    //   WBTC (8 dec) / HD-USDC (18 dec): base_lot_size=1, quote_lot_size=10^10
    //     price=60_000, qty=1 satoshi-lot → 60_000 × 1 × 10^10 = 6×10^14 raw HD-USDC
    //     = $60,000 per BTC ✓
    pub base_lot_size: u64,

    // Raw quote-token units per quote lot. See base_lot_size comment above.
    pub quote_lot_size: u64,

    pub min_order_size: u64,

    pub is_active: bool,

    pub bump: u8,
}
