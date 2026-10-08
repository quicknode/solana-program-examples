use anchor_lang::prelude::*;

use crate::errors::ErrorCode;
use crate::state::Market;

/// Stop the market taking new orders. While `is_active` is false,
/// `place_order` is refused with `MarketPaused`; `cancel_order`,
/// `settle_funds` and `withdraw_fees` do not read the flag, so a pause
/// never stops anyone getting their tokens out. `resume_market` reopens
/// the market.
pub fn handle_pause_market(context: &mut Context<PauseMarketAccountConstraints>) -> Result<()> {
    context.accounts.market.is_active = false;
    Ok(())
}

#[derive(Accounts)]
pub struct PauseMarketAccountConstraints {
    #[account(mut)]
    pub market: BorshAccount<Market>,

    // v1's `has_one = authority` on `market`; in v2 the check lives on the
    // sibling it names.
    #[account(address = market.authority @ ErrorCode::NotMarketAuthority)]
    pub authority: Signer,
}
