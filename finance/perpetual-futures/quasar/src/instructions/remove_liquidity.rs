use {
    crate::{
        constants::MINIMUM_LIQUIDITY,
        instructions::shared::{
            err, error, refresh_price_and_funding_within_band, traders_unrealized_pnl, Rounding,
        },
        state::Pool,
        LpMintPda,
    },
    quasar_lang::cpi::Seed,
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
    let unix_timestamp = accounts.clock.unix_timestamp.get();
    let price = refresh_price_and_funding_within_band(
        &mut accounts.pool,
        &accounts.oracle_feed,
        slot,
        unix_timestamp,
    )?;

    let lp_supply = accounts.lp_mint.supply();
    let traders = traders_unrealized_pnl(
        accounts.pool.long_size.get(),
        accounts.pool.long_size_scaled.get(),
        accounts.pool.short_size.get(),
        accounts.pool.short_size_scaled.get(),
        price,
        // Rounded up, so the pool is valued low and a fraction of a base
        // unit lowers what a share redeems for.
        Rounding::Up,
    )?;
    let aum = (accounts.pool.liquidity.get() as i128)
        .checked_sub(traders)
        .ok_or(ProgramError::ArithmeticOverflow)?;
    if aum <= 0 {
        return Err(err(error::POOL_INSOLVENT));
    }

    // The withheld minimum counts as shares nobody holds, as it does in
    // add_liquidity, so its slice of the pool never leaves.
    let total_shares = (lp_supply as u128)
        .checked_add(MINIMUM_LIQUIDITY as u128)
        .ok_or(ProgramError::ArithmeticOverflow)?;
    let amount_out = (shares as u128)
        .checked_mul(aum as u128)
        .ok_or(ProgramError::ArithmeticOverflow)?
        .checked_div(total_shares)
        .ok_or(ProgramError::ArithmeticOverflow)?;
    let amount_out = u64::try_from(amount_out).map_err(|_| ProgramError::ArithmeticOverflow)?;

    if amount_out == 0 {
        return Err(err(error::AMOUNT_ROUNDS_TO_ZERO));
    }
    // Shares are priced against assets-under-management, which counts traders'
    // unrealized losses as the providers' gain. Those losses are still in the
    // traders' collateral until their positions close, so a withdrawal is
    // capped at `liquidity`, the tokens the providers own now. While traders
    // are up instead, the pricing already keeps a withdrawal below `liquidity`
    // minus their profit, leaving that profit's backing in the pool.
    if amount_out > accounts.pool.liquidity.get() {
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
            &accounts.provider_collateral,
            &accounts.pool,
            amount_out,
            accounts.collateral_mint.decimals(),
        )
        .invoke_signed(seeds)?;

    Ok(())
}
