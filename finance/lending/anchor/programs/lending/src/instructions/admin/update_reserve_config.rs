use anchor_lang::prelude::*;

use crate::state::{LendingMarket, Reserve, ReserveConfig};

pub fn handle_update_reserve_config(
    context: &mut Context<UpdateReserveConfig>,
    config: ReserveConfig,
) -> Result<()> {
    config.validate()?;
    context.accounts.reserve.config = config;
    Ok(())
}

#[derive(Accounts)]
pub struct UpdateReserveConfig {
    // The market is identified by the reserve's `has_one = lending_market`; we
    // only need to prove the signer owns it, not re-derive its address.
    #[account(has_one = owner)]
    pub lending_market: BorshAccount<LendingMarket>,

    pub owner: Signer,

    #[account(
        mut,
        has_one = lending_market,
    )]
    pub reserve: BorshAccount<Reserve>,
}
