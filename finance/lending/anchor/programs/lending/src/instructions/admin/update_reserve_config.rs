use anchor_lang::prelude::*;

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
    // The market is identified by the reserve's `has_one = lending_market`; we
    // only need to prove the signer owns it, not re-derive its address.
    #[account(has_one = owner)]
    pub lending_market: Account<'info, LendingMarket>,

    pub owner: Signer<'info>,

    #[account(
        mut,
        has_one = lending_market,
    )]
    pub reserve: Account<'info, Reserve>,
}
