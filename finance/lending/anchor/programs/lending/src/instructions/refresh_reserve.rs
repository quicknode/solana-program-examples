use anchor_lang::prelude::*;

use crate::state::Reserve;

/// Accrue interest up to the current slot. Must run (as its own instruction in
/// the same transaction) before any handler that reads the reserve's value, and
/// before `refresh_obligation` for any reserve the obligation touches.
pub fn handle_refresh_reserve(context: Context<RefreshReserve>) -> Result<()> {
    context.accounts.reserve.accrue_interest(Clock::get()?.slot)
}

#[derive(Accounts)]
pub struct RefreshReserve<'info> {
    #[account(mut)]
    pub reserve: Account<'info, Reserve>,
}
