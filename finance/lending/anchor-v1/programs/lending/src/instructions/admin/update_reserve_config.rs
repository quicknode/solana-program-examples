use anchor_lang::prelude::*;

use crate::state::{LendingMarket, Reserve, ReserveConfig};

pub fn handle_update_reserve_config(
    context: Context<UpdateReserveConfig>,
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
