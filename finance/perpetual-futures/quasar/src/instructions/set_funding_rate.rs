use {
    crate::{instructions::shared::accrue_funding, state::Pool},
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

/// Retune the pool's funding rate, quoted per second of wall-clock time.
///
/// Funding is accrued at the old rate first, so the seconds already elapsed are
/// charged at the rate that was in force for them rather than repriced by the
/// new one.
#[inline(always)]
pub fn handle_set_funding_rate(
    accounts: &mut SetFundingRate,
    funding_rate_per_second: u64,
) -> Result<(), ProgramError> {
    let pool = &mut accounts.pool;
    accrue_funding(pool, i64::from(Clock::get()?.unix_timestamp))?;
    pool.funding_rate_per_second.set(funding_rate_per_second);
    Ok(())
}
