use anchor_lang::prelude::*;

use crate::constants::POOL_SEED;
use crate::instructions::shared::refresh_price_and_funding;
use crate::state::Pool;

pub fn handle_update_price_average(
    context: Context<UpdatePriceAverageAccountConstraints>,
) -> Result<()> {
    refresh_price_and_funding(&mut context.accounts.pool, &context.accounts.oracle_feed)?;
    Ok(())
}

#[derive(Accounts)]
pub struct UpdatePriceAverageAccountConstraints<'info> {
    /// Anyone may update the average: the result depends only on the oracle
    /// price and the clock, never on who calls.
    pub caller: Signer<'info>,

    #[account(
        mut,
        seeds = [POOL_SEED, pool.collateral_mint.as_ref(), pool.oracle_feed.as_ref()],
        bump = pool.bump,
        has_one = oracle_feed,
    )]
    pub pool: Box<Account<'info, Pool>>,

    /// CHECK: validated by the `has_one = oracle_feed` constraint on the pool.
    pub oracle_feed: UncheckedAccount<'info>,
}
