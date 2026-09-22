use anchor_lang::prelude::*;

use crate::constants::POOL_SEED;
use crate::instructions::shared::accrue_funding;
use crate::state::Pool;

/// Retune the pool's funding rate, quoted per second of wall-clock time.
///
/// Funding is accrued at the old rate first, so the seconds already elapsed are
/// charged at the rate that was in force for them rather than repriced by the
/// new one.
pub fn handle_set_funding_rate(
    context: Context<SetFundingRateAccountConstraints>,
    funding_rate_per_second: u64,
) -> Result<()> {
    let pool = &mut context.accounts.pool;
    accrue_funding(pool, Clock::get()?.unix_timestamp)?;
    pool.funding_rate_per_second = funding_rate_per_second;
    Ok(())
}

#[derive(Accounts)]
pub struct SetFundingRateAccountConstraints<'info> {
    pub authority: Signer<'info>,

    #[account(
        mut,
        seeds = [POOL_SEED, pool.collateral_mint.as_ref(), pool.oracle_feed.as_ref()],
        bump = pool.bump,
        has_one = authority,
    )]
    pub pool: Box<Account<'info, Pool>>,
}
