use {
    quasar_lang::cpi::Seed,
    crate::{
        constants::MINIMUM_LIQUIDITY,
        instructions::shared::{err, error, refresh_price_and_funding, traders_unrealized_pnl},
        state::Pool,
        LpMintPda, PoolAuthorityPda,
    },
    quasar_lang::{prelude::*, sysvars::clock::Clock},
    quasar_spl::prelude::*,
};

#[derive(Accounts)]
pub struct AddLiquidity {
    #[account(mut)]
    pub provider: Signer,
    #[account(
        mut,
        address = Pool::seeds(collateral_mint.address(), oracle_feed.address()),
        has_one(custody_vault),
    )]
    pub pool: Account<Pool>,
    /// Authority PDA over the vault and liquidity-provider mint.
    #[account(address = PoolAuthorityPda::seeds(pool.address()))]
    pub pool_authority: UncheckedAccount,
    /// CHECK: bound to the pool via its seeds (the pool PDA is derived from it).
    pub oracle_feed: UncheckedAccount,
    pub collateral_mint: Account<Mint>,
    #[account(mut, address = LpMintPda::seeds(pool.address()))]
    pub lp_mint: InterfaceAccount<Mint>,
    #[account(mut)]
    pub custody_vault: Account<Token>,
    #[account(mut)]
    pub provider_collateral: Account<Token>,
    #[account(
        mut,
        init(idempotent),
        payer = provider,
        token(mint = lp_mint, authority = provider, token_program = token_program),
    )]
    pub provider_lp: Account<Token>,
    pub token_program: Program<TokenProgram>,
    pub system_program: Program<SystemProgram>,
    pub clock: Sysvar<Clock>,
}

#[inline(always)]
pub fn handle_add_liquidity(
    accounts: &mut AddLiquidity,
    amount: u64,
    minimum_shares_out: u64,
    bumps: &AddLiquidityBumps,
) -> Result<(), ProgramError> {
    if amount == 0 {
        return Err(err(error::ZERO_AMOUNT));
    }

    let slot = accounts.clock.slot.get();
    let price = refresh_price_and_funding(&mut accounts.pool, &accounts.oracle_feed, slot)?;

    let lp_supply = accounts.lp_mint.supply();
    let shares: u64 = if lp_supply == 0 {
        amount
            .checked_sub(MINIMUM_LIQUIDITY)
            .ok_or_else(|| err(error::DEPOSIT_TOO_SMALL))?
    } else {
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
        let computed = (amount as u128)
            .checked_mul(lp_supply as u128)
            .ok_or(ProgramError::ArithmeticOverflow)?
            .checked_div(aum as u128)
            .ok_or(ProgramError::ArithmeticOverflow)?;
        u64::try_from(computed).map_err(|_| ProgramError::ArithmeticOverflow)?
    };

    if shares == 0 {
        return Err(err(error::AMOUNT_ROUNDS_TO_ZERO));
    }
    if shares < minimum_shares_out {
        return Err(err(error::SLIPPAGE_EXCEEDED));
    }

    let new_liquidity = accounts
        .pool
        .liquidity
        .get()
        .checked_add(amount)
        .ok_or(ProgramError::ArithmeticOverflow)?;
    accounts.pool.liquidity.set(new_liquidity);

    accounts
        .token_program
        .transfer_checked(
            &accounts.provider_collateral,
            &accounts.collateral_mint,
            &accounts.custody_vault,
            &accounts.provider,
            amount,
            accounts.collateral_mint.decimals(),
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
        .mint_to(
            &accounts.lp_mint,
            &accounts.provider_lp,
            &accounts.pool_authority,
            shares,
        )
        .invoke_signed(seeds)?;

    Ok(())
}
