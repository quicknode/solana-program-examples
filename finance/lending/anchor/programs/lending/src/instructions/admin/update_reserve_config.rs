use anchor_lang::prelude::*;

use crate::state::{LendingMarket, Reserve, ReserveConfig};

pub fn handle_update_reserve_config(
    context: &mut Context<UpdateReserveConfig>,
    config: ReserveConfig,
) -> Result<()> {
    config.validate()?;
    // Accrue at the old curve first, so the seconds since the last refresh are
    // charged at the rates that applied to them rather than repriced by the new
    // ones.
    let clock = Clock::get()?;
    let reserve = &mut context.accounts.reserve;
    reserve.accrue_interest(clock.slot, clock.unix_timestamp)?;
    reserve.config = config;
    Ok(())
}

#[derive(Accounts)]
pub struct UpdateReserveConfig {
    // The market is identified by `address = reserve.lending_market`; we
    // only need to prove the signer owns it, not re-derive its address.
    #[account(address = reserve.lending_market)]
    pub lending_market: BorshAccount<LendingMarket>,

    #[account(address = lending_market.owner)]
    pub owner: Signer,

    #[account(mut)]
    pub reserve: BorshAccount<Reserve>,
}
