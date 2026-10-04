use anchor_lang::prelude::*;

use crate::errors::ErrorCode;
use crate::state::Market;

/// Reopen a market that `pause_market` stopped: sets `is_active` back to
/// true, so `place_order` accepts orders again.
pub fn handle_resume_market(context: Context<ResumeMarketAccountConstraints>) -> Result<()> {
    context.accounts.market.is_active = true;
    Ok(())
}

#[derive(Accounts)]
pub struct ResumeMarketAccountConstraints<'info> {
    #[account(
        mut,
        has_one = authority @ ErrorCode::NotMarketAuthority,
    )]
    pub market: Account<'info, Market>,

    pub authority: Signer<'info>,
}
