use anchor_lang::prelude::*;

use crate::constants::LENDING_MARKET_SEED;
use crate::state::{LendingMarket, Reserve, ReserveConfig};

pub fn handle_update_reserve_config(
    context: Context<UpdateReserveConfig>,
    config: ReserveConfig,
) -> Result<()> {
    config.validate()?;
    context.accounts.reserve.config = config;
    Ok(())
}

#[derive(Accounts)]
pub struct UpdateReserveConfig<'info> {
    #[account(
        has_one = owner,
        seeds = [LENDING_MARKET_SEED, owner.key().as_ref()],
        bump = lending_market.bump,
    )]
    pub lending_market: Account<'info, LendingMarket>,

    pub owner: Signer<'info>,

    #[account(
        mut,
        has_one = lending_market,
    )]
    pub reserve: Account<'info, Reserve>,
}
