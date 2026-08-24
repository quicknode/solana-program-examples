use anchor_lang::prelude::*;

use crate::constants::{BASIS_POINTS_DENOMINATOR, MARKET_SEED};
use crate::errors::PropAmmError;
use crate::state::Market;

/// Re-quote the market: change the spread, pull the quotes, or restore them.
///
/// For a market-making firm this is the most-used instruction in the program.
/// A fixed spread is only safe while the world is calm; when volatility rises
/// (or the firm's models disagree with the oracle), the rational moves are to
/// widen the spread or stop quoting entirely. Onchain prop AMMs do exactly
/// this — during fast markets their quotes vanish and return minutes later.
pub fn handle_set_quote(
    context: Context<SetQuoteAccountConstraints>,
    spread_bps: u16,
    paused: bool,
) -> Result<()> {
    // Same bounds as at creation: a quote must charge something and the bid
    // must stay positive.
    require!(
        spread_bps > 0 && spread_bps < BASIS_POINTS_DENOMINATOR as u16,
        PropAmmError::InvalidParameter
    );

    let market = &mut context.accounts.market;
    market.spread_bps = spread_bps;
    market.paused = paused;

    Ok(())
}

#[derive(Accounts)]
pub struct SetQuoteAccountConstraints<'info> {
    pub operator: Signer<'info>,

    #[account(
        mut,
        seeds = [MARKET_SEED, market.base_mint.as_ref(), market.quote_mint.as_ref()],
        bump = market.bump,
        has_one = operator,
    )]
    pub market: Box<Account<'info, Market>>,
}
