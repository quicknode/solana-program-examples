use {
    crate::{
        constants::BASIS_POINTS_DENOMINATOR,
        instructions::shared::{err, error},
        state::Market,
    },
    quasar_lang::prelude::*,
};

#[derive(Accounts)]
pub struct SetQuote {
    pub operator: Signer,
    #[account(
        mut,
        address = Market::seeds(base_mint.address(), quote_mint.address()),
        has_one(operator),
    )]
    pub market: Account<Market>,
    /// CHECK: seed input for the market PDA.
    pub base_mint: UncheckedAccount,
    /// CHECK: seed input for the market PDA.
    pub quote_mint: UncheckedAccount,
}

/// Re-quote the market: change the spread, pull the quotes (`paused = 1`), or
/// restore them. For a market-making firm this is the most-used instruction in
/// the program: when volatility rises, the rational moves are to widen the
/// spread or stop quoting entirely.
#[inline(always)]
pub fn handle_set_quote(
    accounts: &mut SetQuote,
    spread_bps: u16,
    paused: u8,
) -> Result<(), ProgramError> {
    // Same bounds as at creation: a quote must charge something and the bid
    // must stay positive.
    if spread_bps == 0 || spread_bps >= BASIS_POINTS_DENOMINATOR as u16 {
        return Err(err(error::INVALID_PARAMETER));
    }
    if paused > 1 {
        return Err(err(error::INVALID_PARAMETER));
    }

    accounts.market.spread_bps.set(spread_bps);
    // Single-byte fields are plain in the zero-copy view, so no setter.
    accounts.market.paused = paused;
    Ok(())
}
