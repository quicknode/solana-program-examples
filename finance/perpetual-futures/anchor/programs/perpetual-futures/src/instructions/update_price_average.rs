use anchor_lang::prelude::*;

use crate::constants::POOL_SEED;
use crate::instructions::shared::refresh_price_and_funding;
use crate::state::Pool;

pub fn handle_update_price_average(
    context: &mut Context<UpdatePriceAverageAccountConstraints>,
) -> Result<()> {
    refresh_price_and_funding(&mut context.accounts.pool, &context.accounts.oracle_feed)?;
    Ok(())
}

#[derive(Accounts)]
pub struct UpdatePriceAverageAccountConstraints {
    /// Anyone may update the average: the result depends only on the oracle
    /// price and the clock, never on who calls.
    pub caller: Signer,

    #[account(
        mut,
        seeds = [POOL_SEED, pool.collateral_mint.as_ref(), pool.oracle_feed.as_ref()],
        bump = pool.bump,
    )]
    pub pool: Box<BorshAccount<Pool>>,

    /// CHECK: validated by the `address = pool.oracle_feed` constraint below.
    #[account(address = pool.oracle_feed)]
    pub oracle_feed: UncheckedAccount,
}
