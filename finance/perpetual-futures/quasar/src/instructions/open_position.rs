use {
    crate::{
        constants::{BASIS_POINTS_DENOMINATOR, SIDE_LONG, SIDE_SHORT},
        instructions::shared::{
            basis_points_of, credit_fee, err, error, refresh_price_and_funding_within_band,
            scale_size,
        },
        state::{Pool, Position, PositionInner},
    },
    quasar_lang::{prelude::*, sysvars::clock::Clock},
    quasar_spl::prelude::*,
};

#[derive(Accounts)]
pub struct OpenPosition {
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
        init,
        payer = owner,
        address = Position::seeds(pool.address(), owner.address()),
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
    pub rent: Sysvar<Rent>,
}

#[inline(always)]
pub fn handle_open_position(
    accounts: &mut OpenPosition,
    side: u8,
    collateral_amount: u64,
    size: u64,
    acceptable_price: u64,
    bumps: &OpenPositionBumps,
) -> Result<(), ProgramError> {
    if side != SIDE_LONG && side != SIDE_SHORT {
        return Err(err(error::INVALID_PARAMETER));
    }
    if collateral_amount == 0 || size == 0 {
        return Err(err(error::ZERO_AMOUNT));
    }

    let slot = accounts.clock.slot.get();
    let unix_timestamp = accounts.clock.unix_timestamp.get();
    let price = refresh_price_and_funding_within_band(
        &mut accounts.pool,
        &accounts.oracle_feed,
        slot,
        unix_timestamp,
    )?;

    if acceptable_price != 0 {
        let acceptable = if side == SIDE_LONG {
            price <= acceptable_price
        } else {
            price >= acceptable_price
        };
        if !acceptable {
            return Err(err(error::SLIPPAGE_EXCEEDED));
        }
    }

    // The open fee is taken out of the posted collateral; the rest backs the
    // position, and the initial margin is measured against this net collateral.
    let open_fee = basis_points_of(size, accounts.pool.open_fee_bps.get())?;
    let net_collateral = collateral_amount
        .checked_sub(open_fee)
        .ok_or_else(|| err(error::INSUFFICIENT_COLLATERAL))?;
    if net_collateral == 0 {
        return Err(err(error::ZERO_AMOUNT));
    }

    // Initial margin: net collateral must be at least `initial_margin_bps` of
    // the notional size, compared as `net_collateral * 10_000 >= size * bps`
    // so nothing is rounded. `initialize_pool` keeps the initial margin above
    // the maintenance margin, so a position that passes this check opens with
    // equity above the liquidation threshold.
    let collateral_scaled = (net_collateral as u128)
        .checked_mul(BASIS_POINTS_DENOMINATOR as u128)
        .ok_or(ProgramError::ArithmeticOverflow)?;
    let required_scaled = (size as u128)
        .checked_mul(accounts.pool.initial_margin_bps.get() as u128)
        .ok_or(ProgramError::ArithmeticOverflow)?;
    if collateral_scaled < required_scaled {
        return Err(err(error::INITIAL_MARGIN_NOT_MET));
    }

    // Nothing is set aside to back this position's profit, and the pool's
    // liquidity does not limit its size: `close_position` pays each winner the
    // fraction of their profit the pool can back (see `haircut_ratio`).
    let size_scaled = scale_size(size, price)?;

    accounts.position.set_inner(PositionInner {
        owner: *accounts.owner.address(),
        pool: *accounts.pool.address(),
        side,
        collateral: net_collateral,
        size,
        entry_price: price,
        size_scaled,
        entry_funding: accounts.pool.cumulative_funding.get(),
        entry_slot: slot,
        bump: bumps.position,
    });

    let new_total_collateral = accounts
        .pool
        .total_collateral
        .get()
        .checked_add(net_collateral)
        .ok_or(ProgramError::ArithmeticOverflow)?;
    accounts.pool.total_collateral.set(new_total_collateral);

    credit_fee(&mut accounts.pool, open_fee)?;

    if side == SIDE_LONG {
        let long_size = accounts
            .pool
            .long_size
            .get()
            .checked_add(size as u128)
            .ok_or(ProgramError::ArithmeticOverflow)?;
        accounts.pool.long_size.set(long_size);
        let long_scaled = accounts
            .pool
            .long_size_scaled
            .get()
            .checked_add(size_scaled)
            .ok_or(ProgramError::ArithmeticOverflow)?;
        accounts.pool.long_size_scaled.set(long_scaled);
    } else {
        let short_size = accounts
            .pool
            .short_size
            .get()
            .checked_add(size as u128)
            .ok_or(ProgramError::ArithmeticOverflow)?;
        accounts.pool.short_size.set(short_size);
        let short_scaled = accounts
            .pool
            .short_size_scaled
            .get()
            .checked_add(size_scaled)
            .ok_or(ProgramError::ArithmeticOverflow)?;
        accounts.pool.short_size_scaled.set(short_scaled);
    }

    accounts
        .token_program
        .transfer_checked(
            &accounts.trader_collateral,
            &accounts.collateral_mint,
            &accounts.custody_vault,
            &accounts.owner,
            collateral_amount,
            accounts.collateral_mint.decimals(),
        )
        .invoke()?;

    Ok(())
}
