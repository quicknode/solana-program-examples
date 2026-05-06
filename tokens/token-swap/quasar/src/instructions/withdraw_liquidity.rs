use {
    crate::{
        state::{Amm, Pool},
        AmmPda, LiquidityMintPda, PoolAuthorityPda, PoolPda,
    },
    quasar_lang::prelude::*,
    quasar_spl::prelude::*,
};

/// Accounts for withdrawing liquidity from a pool.
#[derive(Accounts)]
pub struct WithdrawLiquidity {
    #[account(address = AmmPda::seeds())]
    pub amm: Account<Amm>,
    #[account(address = PoolPda::seeds(amm.address(), mint_a.address(), mint_b.address()))]
    pub pool: Account<Pool>,
    /// Pool authority PDA.
    #[account(address = PoolAuthorityPda::seeds(amm.address(), mint_a.address(), mint_b.address()))]
    pub pool_authority: UncheckedAccount,
    pub depositor: Signer,
    #[account(mut, address = LiquidityMintPda::seeds(amm.address(), mint_a.address(), mint_b.address()))]
    pub mint_liquidity: Account<Mint>,
    #[account(mut)]
    pub mint_a: Account<Mint>,
    #[account(mut)]
    pub mint_b: Account<Mint>,
    #[account(mut)]
    pub pool_account_a: Account<Token>,
    #[account(mut)]
    pub pool_account_b: Account<Token>,
    #[account(mut)]
    pub depositor_account_liquidity: Account<Token>,
    #[account(
        mut,
        init(idempotent),
        payer = payer,
        token(mint = mint_a, authority = depositor, token_program = token_program),
    )]
    pub depositor_account_a: Account<Token>,
    #[account(
        mut,
        init(idempotent),
        payer = payer,
        token(mint = mint_b, authority = depositor, token_program = token_program),
    )]
    pub depositor_account_b: Account<Token>,
    #[account(mut)]
    pub payer: Signer,
    pub token_program: Program<TokenProgram>,
    pub system_program: Program<SystemProgram>,
}

#[inline(always)]
pub fn handle_withdraw_liquidity(
    accounts: &mut WithdrawLiquidity,
    amount: u64,
    bumps: &WithdrawLiquidityBumps,
) -> Result<(), ProgramError> {
    // Seed order matches PoolAuthorityPda: [b"authority", amm, mint_a, mint_b, bump].
    let bump = [bumps.pool_authority];
    let seeds: &[Seed] = &[
        Seed::from(crate::AUTHORITY_SEED),
        Seed::from(accounts.amm.address().as_ref()),
        Seed::from(accounts.mint_a.address().as_ref()),
        Seed::from(accounts.mint_b.address().as_ref()),
        Seed::from(&bump as &[u8]),
    ];

    // Compute proportional amounts.
    let total_liquidity = accounts.mint_liquidity.supply() + crate::MINIMUM_LIQUIDITY;

    let amount_a = (amount as u128)
        .checked_mul(accounts.pool_account_a.amount() as u128)
        .ok_or(ProgramError::ArithmeticOverflow)?
        .checked_div(total_liquidity as u128)
        .ok_or(ProgramError::ArithmeticOverflow)? as u64;

    let amount_b = (amount as u128)
        .checked_mul(accounts.pool_account_b.amount() as u128)
        .ok_or(ProgramError::ArithmeticOverflow)?
        .checked_div(total_liquidity as u128)
        .ok_or(ProgramError::ArithmeticOverflow)? as u64;

    // Transfer token A from pool to depositor.
    accounts.token_program
        .transfer(&accounts.pool_account_a, &accounts.depositor_account_a, &accounts.pool_authority, amount_a)
        .invoke_signed(seeds)?;

    // Transfer token B from pool to depositor.
    accounts.token_program
        .transfer(&accounts.pool_account_b, &accounts.depositor_account_b, &accounts.pool_authority, amount_b)
        .invoke_signed(seeds)?;

    // Burn LP tokens.
    accounts.token_program
        .burn(&accounts.depositor_account_liquidity, &accounts.mint_liquidity, &accounts.depositor, amount)
        .invoke()?;

    Ok(())
}
