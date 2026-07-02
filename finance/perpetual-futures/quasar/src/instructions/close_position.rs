use {
    crate::{
        constants::SIDE_LONG,
        instructions::shared::{
            apply_haircut, basis_points_of, err, error, haircut_ratio, position_funding,
            position_pnl, refresh_price_and_funding, split_fee,
        },
        state::{Pool, Position},
        PoolAuthorityPda,
    },
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
    #[account(address = PoolAuthorityPda::seeds(pool.address()))]
    pub pool_authority: UncheckedAccount,
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
    let price = refresh_price_and_funding(&mut accounts.pool, &accounts.oracle_feed, slot)?;

    // Compute the haircut against the whole pool before this position leaves the
    // accumulators, so the closer is one of the winners being scaled.
    let haircut = haircut_ratio(
        accounts.pool.liquidity.get(),
        accounts.pool.insurance_fund.get(),
        accounts.pool.long_size.get(),
        accounts.pool.long_size_scaled.get(),
        accounts.pool.short_size.get(),
        accounts.pool.short_size_scaled.get(),
        price,
    )?;

    let side = accounts.position.side;
    let size = accounts.position.size.get();
    let entry_price = accounts.position.entry_price.get();
    let collateral = accounts.position.collateral.get();
    let size_scaled = accounts.position.size_scaled.get();
    let entry_funding = accounts.position.entry_funding.get();
    let entry_slot = accounts.position.entry_slot.get();

    let pnl = position_pnl(side, size, entry_price, price)?;
    let funding = position_funding(
        side,
        size,
        entry_funding,
        accounts.pool.cumulative_funding.get(),
    )?;
    // Profit is a junior claim, gated twice; a loss settles in full. It must
    // have matured (the warm-up since open elapsed), then it is haircut to the
    // fraction `h` the pool can back.
    let realized_pnl = if pnl > 0 {
        if slot < entry_slot.saturating_add(accounts.pool.profit_warmup_slots.get()) {
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

    let (insurance_cut, protocol_cut) = split_fee(close_fee, accounts.pool.insurance_fee_bps.get())?;

    remove_open_interest(&mut accounts.pool, side, size, size_scaled)?;

    let new_total_collateral = accounts
        .pool
        .total_collateral
        .get()
        .checked_sub(collateral)
        .ok_or(ProgramError::ArithmeticOverflow)?;
    accounts.pool.total_collateral.set(new_total_collateral);

    let liquidity_delta = funding
        .checked_sub(realized_pnl)
        .ok_or(ProgramError::ArithmeticOverflow)?;
    let new_liquidity = (accounts.pool.liquidity.get() as i128)
        .checked_add(liquidity_delta)
        .ok_or(ProgramError::ArithmeticOverflow)?;
    if new_liquidity < 0 {
        return Err(err(error::POOL_INSOLVENT));
    }
    accounts
        .pool
        .liquidity
        .set(u64::try_from(new_liquidity).map_err(|_| ProgramError::ArithmeticOverflow)?);

    let new_protocol_fees = accounts
        .pool
        .protocol_fees
        .get()
        .checked_add(protocol_cut)
        .ok_or(ProgramError::ArithmeticOverflow)?;
    accounts.pool.protocol_fees.set(new_protocol_fees);

    let new_insurance_fund = accounts
        .pool
        .insurance_fund
        .get()
        .checked_add(insurance_cut)
        .ok_or(ProgramError::ArithmeticOverflow)?;
    accounts.pool.insurance_fund.set(new_insurance_fund);

    let bump = [bumps.pool_authority];
    let seeds: &[Seed] = &[
        Seed::from(b"authority".as_ref()),
        Seed::from(accounts.pool.address().as_ref()),
        Seed::from(&bump as &[u8]),
    ];
    accounts
        .token_program
        .transfer(
            &accounts.custody_vault,
            &accounts.trader_collateral,
            &accounts.pool_authority,
            payout,
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
