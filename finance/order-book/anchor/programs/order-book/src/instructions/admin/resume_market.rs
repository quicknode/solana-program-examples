use anchor_lang::prelude::*;

use crate::errors::ErrorCode;
use crate::state::Market;

/// Reopen a market that `pause_market` stopped: sets `is_active` back to
/// true, so `place_order` accepts orders again.
pub fn handle_resume_market(context: &mut Context<ResumeMarketAccountConstraints>) -> Result<()> {
    context.accounts.market.is_active = true;
    Ok(())
}

#[derive(Accounts)]
pub struct ResumeMarketAccountConstraints {
    #[account(mut)]
    pub market: BorshAccount<Market>,

    // v1's `has_one = authority` on `market`; in v2 the check lives on the
    // sibling it names.
    #[account(address = market.authority @ ErrorCode::NotMarketAuthority)]
    pub authority: Signer,
}
