use quasar_lang::prelude::*;

use crate::errors::OrderBookError;
use crate::state::Market;

/// Stop the market taking new orders. While `is_active` is false,
/// `place_order` is refused with `MarketPaused`; `cancel_order`,
/// `settle_funds` and `withdraw_fees` do not read the flag, so a pause
/// never stops anyone getting their tokens out. `resume_market` reopens
/// the market.
#[derive(Accounts)]
pub struct PauseMarketAccountConstraints {
    #[account(mut, has_one(authority) @ OrderBookError::NotMarketAuthority)]
    pub market: Account<Market>,

    pub authority: Signer,
}

#[inline(always)]
pub fn handle_pause_market(
    accounts: &mut PauseMarketAccountConstraints,
) -> Result<(), ProgramError> {
    accounts.market.is_active = PodBool::from(false);
    Ok(())
}
