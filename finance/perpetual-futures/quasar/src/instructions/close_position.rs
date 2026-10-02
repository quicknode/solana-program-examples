use {
    crate::{
        constants::SIDE_LONG,
        instructions::shared::{
            apply_haircut, basis_points_of, credit_fee, err, error, haircut_ratio,
            position_funding, position_pnl, refresh_price_and_funding_within_band,
        },
        state::{Pool, Position},
    },
    quasar_lang::cpi::Seed,
    quasar_lang::{prelude::*, sysvars::clock::Clock},
    quasar_spl::prelude::*,
};

#[derive(Accounts)]
pub struct ClosePosition {
    #[account(mut)]
    pub owner: Signer,
    #[account(
        mut,
        address = Pool::seeds(collateral_mint.address(), oracle_feed.address()),
        has_one(custody_vault),
    )]
    pub pool: Account<Pool>,
    #[account(
        mut,
        has_one(owner),
        address = Position::seeds(pool.address(), owner.address()),
        close(dest = owner),
    )]
    pub position: Account<Position>,
    /// CHECK: bound to the pool via its seeds.
    pub oracle_feed: UncheckedAccount,
    pub collateral_mint: Account<Mint>,
    #[account(mut)]
    pub custody_vault: Account<Token>,
    #[account(mut)]
    pub trader_collateral: Account<Token>,
    pub token_program: Program<TokenProgram>,
    pub system_program: Program<SystemProgram>,
    pub clock: Sysvar<Clock>,
}

#[inline(always)]
pub fn handle_close_position(
    accounts: &mut ClosePosition,
    minimum_payout: u64,
    bumps: &ClosePositionBumps,
) -> Result<(), ProgramError> {
    let slot = accounts.clock.slot.get();
    let unix_timestamp = accounts.clock.unix_timestamp.get();
    let price = refresh_price_and_funding_within_band(
        &mut accounts.pool,
        &accounts.oracle_feed,
        slot,
        unix_timestamp,
    )?;

    let side = accounts.position.side;
    let size = accounts.position.size.get();
    let entry_price = accounts.position.entry_price.get();
    let collateral = accounts.position.collateral.get();
    let size_scaled = accounts.position.size_scaled.get();
    let entry_funding = accounts.position.entry_funding.get();
    let entry_slot = accounts.position.entry_slot.get();

    let pnl = position_pnl(side, size, entry_price, price)?;
    // The haircut is computed while this position is still in the per-side
    // accumulators, so its own profit counts toward the liability and it is
    // paid the same fraction as any other winner closing at this price. Its
    // own profit is passed too: if open losers offset it in the aggregate,
    // the haircut is sized against that profit, so the payout is at most the
    // backing and the close is never refused for lack of it.
    let haircut = haircut_ratio(&accounts.pool, price, pnl)?;
    let funding = position_funding(
        side,
        size,
        entry_funding,
        accounts.pool.cumulative_funding.get(),
    )?;
    // A profit is paid only once the position has been open for the pool's
    // warm-up, and then only the haircut fraction of it. A loss settles in
    // full, at any time.
    let realized_pnl = if pnl > 0 {
        let matured_at = entry_slot
            .checked_add(accounts.pool.profit_warmup_slots.get())
            .ok_or(ProgramError::ArithmeticOverflow)?;
        if slot < matured_at {
            return Err(err(error::PROFIT_NOT_MATURED));
        }
        apply_haircut(pnl, haircut)?
    } else {
        pnl
    };
    let equity = (collateral as i128)
        .checked_add(realized_pnl)
        .ok_or(ProgramError::ArithmeticOverflow)?
        .checked_sub(funding)
        .ok_or(ProgramError::ArithmeticOverflow)?;

    let close_fee = basis_points_of(size, accounts.pool.close_fee_bps.get())?;
    let payout = equity
        .checked_sub(close_fee as i128)
        .ok_or(ProgramError::ArithmeticOverflow)?;
    if payout <= 0 {
        return Err(err(error::POSITION_NOT_HEALTHY));
    }
    let payout = u64::try_from(payout).map_err(|_| ProgramError::ArithmeticOverflow)?;
    if payout < minimum_payout {
        return Err(err(error::SLIPPAGE_EXCEEDED));
    }

    remove_open_interest(&mut accounts.pool, side, size, size_scaled)?;

    let new_total_collateral = accounts
        .pool
        .total_collateral
        .get()
        .checked_sub(collateral)
        .ok_or(ProgramError::ArithmeticOverflow)?;
    accounts.pool.total_collateral.set(new_total_collateral);

    // Liquidity providers are the counterparty: they pay the trader's
    // haircut profit and receive their loss, and collect the funding the
    // trader owed. The part of a profit the haircut withholds stays in
    // `liquidity`. A payment larger than `liquidity` takes the rest from the
    // insurance fund, which the haircut counted as backing. The haircut keeps
    // the profit within both; `POOL_INSOLVENT` remains as a defensive check.
    let liquidity_delta = funding
        .checked_sub(realized_pnl)
        .ok_or(ProgramError::ArithmeticOverflow)?;
    let new_liquidity = (accounts.pool.liquidity.get() as i128)
        .checked_add(liquidity_delta)
        .ok_or(ProgramError::ArithmeticOverflow)?;
    if new_liquidity < 0 {
        let shortfall = u64::try_from(new_liquidity.unsigned_abs())
            .map_err(|_| ProgramError::ArithmeticOverflow)?;
        let new_insurance_fund = accounts
            .pool
            .insurance_fund
            .get()
            .checked_sub(shortfall)
            .ok_or_else(|| err(error::POOL_INSOLVENT))?;
        accounts.pool.insurance_fund.set(new_insurance_fund);
        accounts.pool.liquidity.set(0);
    } else {
        accounts
            .pool
            .liquidity
            .set(u64::try_from(new_liquidity).map_err(|_| ProgramError::ArithmeticOverflow)?);
    }
    credit_fee(&mut accounts.pool, close_fee)?;

    // The pool signs the CPI below with its own seeds.
    let bump = [bumps.pool];
    let seeds: &[Seed] = &[
        Seed::from(b"pool".as_ref()),
        Seed::from(accounts.collateral_mint.address().as_ref()),
        Seed::from(accounts.oracle_feed.address().as_ref()),
        Seed::from(&bump as &[u8]),
    ];
    accounts
        .token_program
        .transfer_checked(
            &accounts.custody_vault,
            &accounts.collateral_mint,
            &accounts.trader_collateral,
            &accounts.pool,
            payout,
            accounts.collateral_mint.decimals(),
        )
        .invoke_signed(seeds)?;

    Ok(())
}

/// Subtract a position's open interest from the pool's per-side accumulators.
pub fn remove_open_interest(
    pool: &mut Account<Pool>,
    side: u8,
    size: u64,
    size_scaled: u128,
) -> Result<(), ProgramError> {
    if side == SIDE_LONG {
        let long_size = pool
            .long_size
            .get()
            .checked_sub(size as u128)
            .ok_or(ProgramError::ArithmeticOverflow)?;
        pool.long_size.set(long_size);
        let long_scaled = pool
            .long_size_scaled
            .get()
            .checked_sub(size_scaled)
            .ok_or(ProgramError::ArithmeticOverflow)?;
        pool.long_size_scaled.set(long_scaled);
    } else {
        let short_size = pool
            .short_size
            .get()
            .checked_sub(size as u128)
            .ok_or(ProgramError::ArithmeticOverflow)?;
        pool.short_size.set(short_size);
        let short_scaled = pool
            .short_size_scaled
            .get()
            .checked_sub(size_scaled)
            .ok_or(ProgramError::ArithmeticOverflow)?;
        pool.short_size_scaled.set(short_scaled);
    }
    Ok(())
}
