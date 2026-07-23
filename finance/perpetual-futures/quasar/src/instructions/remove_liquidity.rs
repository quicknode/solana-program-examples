use {
    quasar_lang::cpi::Seed,
    crate::{
        instructions::shared::{err, error, refresh_price_and_funding, traders_unrealized_pnl},
        state::Pool,
        LpMintPda, PoolAuthorityPda,
    },
    quasar_lang::{prelude::*, sysvars::clock::Clock},
    quasar_spl::prelude::*,
};

#[derive(Accounts)]
pub struct RemoveLiquidity {
    #[account(mut)]
    pub provider: Signer,
    #[account(
        mut,
        address = Pool::seeds(collateral_mint.address(), oracle_feed.address()),
        has_one(custody_vault),
    )]
    pub pool: Account<Pool>,
    #[account(address = PoolAuthorityPda::seeds(pool.address()))]
    pub pool_authority: UncheckedAccount,
    /// CHECK: bound to the pool via its seeds.
    pub oracle_feed: UncheckedAccount,
    pub collateral_mint: Account<Mint>,
    #[account(mut, address = LpMintPda::seeds(pool.address()))]
    pub lp_mint: InterfaceAccount<Mint>,
    #[account(mut)]
    pub custody_vault: Account<Token>,
    #[account(
        mut,
        init(idempotent),
        payer = provider,
        token(mint = collateral_mint, authority = provider, token_program = token_program),
    )]
    pub provider_collateral: Account<Token>,
    #[account(mut)]
    pub provider_lp: Account<Token>,
    pub token_program: Program<TokenProgram>,
    pub system_program: Program<SystemProgram>,
    pub clock: Sysvar<Clock>,
}

#[inline(always)]
pub fn handle_remove_liquidity(
    accounts: &mut RemoveLiquidity,
    shares: u64,
    minimum_amount_out: u64,
    bumps: &RemoveLiquidityBumps,
) -> Result<(), ProgramError> {
    if shares == 0 {
        return Err(err(error::ZERO_AMOUNT));
    }

    let slot = accounts.clock.slot.get();
    let price = refresh_price_and_funding(&mut accounts.pool, &accounts.oracle_feed, slot)?;

    let lp_supply = accounts.lp_mint.supply();
    let traders = traders_unrealized_pnl(
        accounts.pool.long_size.get(),
        accounts.pool.long_size_scaled.get(),
        accounts.pool.short_size.get(),
        accounts.pool.short_size_scaled.get(),
        price,
    )?;
    let aum = (accounts.pool.liquidity.get() as i128)
        .checked_sub(traders)
        .ok_or(ProgramError::ArithmeticOverflow)?;
    if aum <= 0 {
        return Err(err(error::POOL_INSOLVENT));
    }

    let amount_out = (shares as u128)
        .checked_mul(aum as u128)
        .ok_or(ProgramError::ArithmeticOverflow)?
        .checked_div(lp_supply as u128)
        .ok_or(ProgramError::ArithmeticOverflow)?;
    let amount_out = u64::try_from(amount_out).map_err(|_| ProgramError::ArithmeticOverflow)?;

    if amount_out == 0 {
        return Err(err(error::AMOUNT_ROUNDS_TO_ZERO));
    }
    // Only free liquidity can leave; the reserved portion backs open positions.
    let free_liquidity = accounts
        .pool
        .liquidity
        .get()
        .checked_sub(accounts.pool.reserved_liquidity.get())
        .ok_or(ProgramError::ArithmeticOverflow)?;
    if amount_out > free_liquidity {
        return Err(err(error::INSUFFICIENT_LIQUIDITY));
    }
    if amount_out < minimum_amount_out {
        return Err(err(error::SLIPPAGE_EXCEEDED));
    }

    let new_liquidity = accounts
        .pool
        .liquidity
        .get()
        .checked_sub(amount_out)
        .ok_or(ProgramError::ArithmeticOverflow)?;
    accounts.pool.liquidity.set(new_liquidity);

    accounts
        .token_program
        .burn(
            &accounts.provider_lp,
            &accounts.lp_mint,
            &accounts.provider,
            shares,
        )
        .invoke()?;

    let bump = [bumps.pool_authority];
    let seeds: &[Seed] = &[
        Seed::from(b"authority".as_ref()),
        Seed::from(accounts.pool.address().as_ref()),
        Seed::from(&bump as &[u8]),
    ];
    accounts
        .token_program
        .transfer_checked(
            &accounts.custody_vault,
            &accounts.collateral_mint,
            &accounts.provider_collateral,
            &accounts.pool_authority,
            amount_out,
            accounts.collateral_mint.decimals(),
        )
        .invoke_signed(seeds)?;

    Ok(())
}
