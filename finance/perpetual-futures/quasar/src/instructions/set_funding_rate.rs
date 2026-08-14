use {
    crate::{instructions::shared::advance_funding, state::Pool},
    quasar_lang::{prelude::*, sysvars::Sysvar},
};

#[derive(Accounts)]
pub struct SetFundingRate {
    pub authority: Signer,
    #[account(
        mut,
        has_one(authority),
        address = Pool::seeds(collateral_mint.address(), oracle_feed.address()),
    )]
    pub pool: Account<Pool>,
    /// CHECK: bound to the pool via its seeds.
    pub collateral_mint: UncheckedAccount,
    /// CHECK: bound to the pool via its seeds.
    pub oracle_feed: UncheckedAccount,
}

/// Retune the pool's funding rate. The rate is quoted per slot, so what holding
/// a position costs per hour depends on the cluster's slot time as well as on
/// this number: shorten the slot and the same rate charges the heavier side
/// more. Solana lowers the slot time over time, so a pool that outlives a
/// reduction needs its rate brought back in line.
///
/// Funding advances at the old rate first, so slots already elapsed are charged
/// at the rate that was in force for them rather than repriced by the new one.
#[inline(always)]
pub fn handle_set_funding_rate(
    accounts: &mut SetFundingRate,
    funding_rate_per_slot: u64,
) -> Result<(), ProgramError> {
    let pool = &mut accounts.pool;
    let slot = u64::from(Clock::get()?.slot);

    let new_funding = advance_funding(
        pool.cumulative_funding.get(),
        pool.last_funding_slot.get(),
        slot,
        pool.funding_rate_per_slot.get(),
        pool.long_size.get(),
        pool.short_size.get(),
    )?;
    pool.cumulative_funding.set(new_funding);
    pool.last_funding_slot.set(slot);
    pool.funding_rate_per_slot.set(funding_rate_per_slot);
    Ok(())
}
