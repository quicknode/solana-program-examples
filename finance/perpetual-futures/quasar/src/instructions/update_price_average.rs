use {
    crate::{instructions::shared::refresh_price_and_funding, state::Pool},
    quasar_lang::{prelude::*, sysvars::clock::Clock},
    quasar_spl::prelude::*,
};

#[derive(Accounts)]
pub struct UpdatePriceAverage {
    /// Anyone may update the average: the result depends only on the oracle
    /// price and the clock, never on who calls.
    pub caller: Signer,
    #[account(
        mut,
        address = Pool::seeds(collateral_mint.address(), oracle_feed.address()),
    )]
    pub pool: Account<Pool>,
    /// CHECK: bound to the pool via its seeds.
    pub oracle_feed: UncheckedAccount,
    pub collateral_mint: Account<Mint>,
    pub clock: Sysvar<Clock>,
}

#[inline(always)]
pub fn handle_update_price_average(accounts: &mut UpdatePriceAverage) -> Result<(), ProgramError> {
    let slot = accounts.clock.slot.get();
    let unix_timestamp = accounts.clock.unix_timestamp.get();
    refresh_price_and_funding(
        &mut accounts.pool,
        &accounts.oracle_feed,
        slot,
        unix_timestamp,
    )?;
    Ok(())
}
