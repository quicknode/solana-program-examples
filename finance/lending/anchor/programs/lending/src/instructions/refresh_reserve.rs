use anchor_lang::prelude::*;

use crate::state::Reserve;

/// Accrue interest up to the current time. Must run (as its own instruction in
/// the same transaction) before any handler that reads the reserve's value, and
/// before `refresh_obligation` for any reserve the obligation touches.
pub fn handle_refresh_reserve(context: &mut Context<RefreshReserve>) -> Result<()> {
    let clock = Clock::get()?;
    context
        .accounts
        .reserve
        .accrue_interest(clock.slot, clock.unix_timestamp)
}

#[derive(Accounts)]
pub struct RefreshReserve {
    #[account(mut)]
    pub reserve: BorshAccount<Reserve>,
}
