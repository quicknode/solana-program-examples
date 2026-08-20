use {
    crate::{
        constants::{SIDE_LONG, SIDE_SHORT},
        instructions::shared::{
            basis_points_of, err, error, refresh_price_and_funding, scale_size,
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
    let price = refresh_price_and_funding(&mut accounts.pool, &accounts.oracle_feed, slot)?;

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

    let open_fee = basis_points_of(size, accounts.pool.open_fee_bps.get())?;
    let net_collateral = collateral_amount
        .checked_sub(open_fee)
        .ok_or_else(|| err(error::INSUFFICIENT_LIQUIDITY))?;
    if net_collateral == 0 {
        return Err(err(error::ZERO_AMOUNT));
    }

    let max_notional = (net_collateral as u128)
        .checked_mul(accounts.pool.max_leverage.get() as u128)
        .ok_or(ProgramError::ArithmeticOverflow)?;
    if size as u128 > max_notional {
        return Err(err(error::LEVERAGE_TOO_HIGH));
    }

    let maintenance = basis_points_of(size, accounts.pool.maintenance_margin_bps.get())?;
    if net_collateral <= maintenance {
        return Err(err(error::POSITION_NOT_HEALTHY));
    }

    // Reserve liquidity to cover this position's maximum recoverable profit
    // (its notional `size`), backed by liquidity-provider capital. This also
    // caps total open interest at the pool's liquidity.
    let new_reserved = accounts
        .pool
        .reserved_liquidity
        .get()
        .checked_add(size)
        .ok_or(ProgramError::ArithmeticOverflow)?;
    if new_reserved > accounts.pool.liquidity.get() {
        return Err(err(error::INSUFFICIENT_LIQUIDITY));
    }
    accounts.pool.reserved_liquidity.set(new_reserved);

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
        bump: bumps.position,
    });

    let new_total_collateral = accounts
        .pool
        .total_collateral
        .get()
        .checked_add(net_collateral)
        .ok_or(ProgramError::ArithmeticOverflow)?;
    accounts.pool.total_collateral.set(new_total_collateral);

    let new_protocol_fees = accounts
        .pool
        .protocol_fees
        .get()
        .checked_add(open_fee)
        .ok_or(ProgramError::ArithmeticOverflow)?;
    accounts.pool.protocol_fees.set(new_protocol_fees);

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
