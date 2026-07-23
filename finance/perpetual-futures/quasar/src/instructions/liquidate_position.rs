use {
    quasar_lang::cpi::Seed,
    crate::{
        instructions::{
            close_position::remove_open_interest,
            shared::{
                basis_points_of, err, error, position_funding, position_pnl,
                refresh_price_and_funding,
            },
        },
        state::{Pool, Position},
        PoolAuthorityPda,
    },
    quasar_lang::{prelude::*, sysvars::clock::Clock},
    quasar_spl::prelude::*,
};

#[derive(Accounts)]
pub struct LiquidatePosition {
    #[account(mut)]
    pub liquidator: Signer,
    /// CHECK: the position owner; receives the rent refund and any equity left.
    #[account(mut)]
    pub owner: UncheckedAccount,
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
    #[account(
        mut,
        init(idempotent),
        payer = liquidator,
        token(mint = collateral_mint, authority = liquidator, token_program = token_program),
    )]
    pub liquidator_collateral: Account<Token>,
    pub token_program: Program<TokenProgram>,
    pub system_program: Program<SystemProgram>,
    pub clock: Sysvar<Clock>,
}

#[inline(always)]
pub fn handle_liquidate_position(
    accounts: &mut LiquidatePosition,
    bumps: &LiquidatePositionBumps,
) -> Result<(), ProgramError> {
    let slot = accounts.clock.slot.get();
    let price = refresh_price_and_funding(&mut accounts.pool, &accounts.oracle_feed, slot)?;

    let side = accounts.position.side;
    let size = accounts.position.size.get();
    let entry_price = accounts.position.entry_price.get();
    let collateral = accounts.position.collateral.get();
    let size_scaled = accounts.position.size_scaled.get();
    let entry_funding = accounts.position.entry_funding.get();

    let pnl = position_pnl(side, size, entry_price, price)?;
    let funding = position_funding(
        side,
        size,
        entry_funding,
        accounts.pool.cumulative_funding.get(),
    )?;
    let equity = (collateral as i128)
        .checked_add(pnl)
        .ok_or(ProgramError::ArithmeticOverflow)?
        .checked_sub(funding)
        .ok_or(ProgramError::ArithmeticOverflow)?;

    let maintenance = basis_points_of(size, accounts.pool.maintenance_margin_bps.get())?;
    if equity > maintenance as i128 {
        return Err(err(error::POSITION_HEALTHY));
    }

    let remaining_equity =
        u64::try_from(equity.max(0)).map_err(|_| ProgramError::ArithmeticOverflow)?;
    let liquidation_fee = basis_points_of(size, accounts.pool.liquidation_fee_bps.get())?;
    let liquidator_payout = liquidation_fee.min(remaining_equity);
    let trader_refund = remaining_equity
        .checked_sub(liquidator_payout)
        .ok_or(ProgramError::ArithmeticOverflow)?;

    remove_open_interest(&mut accounts.pool, side, size, size_scaled)?;

    // Release the position's reserved liquidity now that it is closing.
    let new_reserved = accounts
        .pool
        .reserved_liquidity
        .get()
        .checked_sub(size)
        .ok_or(ProgramError::ArithmeticOverflow)?;
    accounts.pool.reserved_liquidity.set(new_reserved);

    let new_total_collateral = accounts
        .pool
        .total_collateral
        .get()
        .checked_sub(collateral)
        .ok_or(ProgramError::ArithmeticOverflow)?;
    accounts.pool.total_collateral.set(new_total_collateral);

    // The pool keeps the position's collateral minus whatever equity is paid out.
    let liquidity_delta = (collateral as i128)
        .checked_sub(remaining_equity as i128)
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

    let bump = [bumps.pool_authority];
    let seeds: &[Seed] = &[
        Seed::from(b"authority".as_ref()),
        Seed::from(accounts.pool.address().as_ref()),
        Seed::from(&bump as &[u8]),
    ];

    if liquidator_payout > 0 {
        accounts
            .token_program
            .transfer_checked(
                &accounts.custody_vault,
                &accounts.collateral_mint,
                &accounts.liquidator_collateral,
                &accounts.pool_authority,
                liquidator_payout,
                accounts.collateral_mint.decimals(),
            )
            .invoke_signed(seeds)?;
    }
    if trader_refund > 0 {
        accounts
            .token_program
            .transfer_checked(
                &accounts.custody_vault,
                &accounts.collateral_mint,
                &accounts.trader_collateral,
                &accounts.pool_authority,
                trader_refund,
                accounts.collateral_mint.decimals(),
            )
            .invoke_signed(seeds)?;
    }

    Ok(())
}
