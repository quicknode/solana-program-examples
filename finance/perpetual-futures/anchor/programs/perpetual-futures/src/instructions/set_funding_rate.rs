use anchor_lang::prelude::*;

use crate::constants::POOL_SEED;
use crate::instructions::shared::accrue_funding;
use crate::state::Pool;

/// Retune the pool's funding rate. The rate is quoted per slot, so the wall-clock
/// cost of holding a position depends on the cluster's slot time as well as on
/// this number: shorten the slot and the same rate charges the heavier side more
/// per hour. Solana lowers the slot time over time, so a pool that outlives a
/// reduction needs its rate brought back in line.
///
/// Funding is accrued at the old rate first, so the slots already elapsed are
/// charged at the rate that was in force for them rather than repriced by the
/// new one.
pub fn handle_set_funding_rate(
    context: &mut Context<SetFundingRateAccountConstraints>,
    funding_rate_per_slot: u64,
) -> Result<()> {
    let pool = &mut context.accounts.pool;
    accrue_funding(pool, Clock::get()?.slot)?;
    pool.funding_rate_per_slot = funding_rate_per_slot;
    Ok(())
}

#[derive(Accounts)]
pub struct SetFundingRateAccountConstraints {
    #[account(address = pool.authority)]
    pub authority: Signer,

    #[account(
        mut,
        seeds = [POOL_SEED, pool.collateral_mint.as_ref(), pool.oracle_feed.as_ref()],
        bump = pool.bump,
    )]
    pub pool: Box<BorshAccount<Pool>>,
}
