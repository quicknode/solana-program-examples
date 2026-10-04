use quasar_lang::prelude::*;

use crate::errors::OrderBookError;
use crate::state::Market;

/// Reopen a market that `pause_market` stopped: sets `is_active` back to
/// true, so `place_order` accepts orders again.
#[derive(Accounts)]
pub struct ResumeMarketAccountConstraints {
    #[account(mut, has_one(authority) @ OrderBookError::NotMarketAuthority)]
    pub market: Account<Market>,

    pub authority: Signer,
}

#[inline(always)]
pub fn handle_resume_market(
    accounts: &mut ResumeMarketAccountConstraints,
) -> Result<(), ProgramError> {
    accounts.market.is_active = PodBool::from(true);
    Ok(())
}
