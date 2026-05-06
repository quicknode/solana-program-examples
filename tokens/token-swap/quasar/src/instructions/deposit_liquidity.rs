use {
    crate::{
        state::{Amm, Pool},
        AmmPda, LiquidityMintPda, PoolAuthorityPda, PoolPda,
    },
    quasar_lang::prelude::*,
    quasar_spl::prelude::*,
};

/// Accounts for depositing liquidity into a pool.
///
/// Seeds reference the amm, mint_a, and mint_b account addresses — these
/// must be provided as separate account inputs.
#[derive(Accounts)]
pub struct DepositLiquidity {
    #[account(address = AmmPda::seeds())]
    pub amm: Account<Amm>,
    #[account(address = PoolPda::seeds(amm.address(), mint_a.address(), mint_b.address()))]
    pub pool: Account<Pool>,
    /// Pool authority PDA.
    #[account(address = PoolAuthorityPda::seeds(amm.address(), mint_a.address(), mint_b.address()))]
    pub pool_authority: UncheckedAccount,
    /// Depositor (must be signer to authorise transfers).
    pub depositor: Signer,
    #[account(mut, address = LiquidityMintPda::seeds(amm.address(), mint_a.address(), mint_b.address()))]
    pub mint_liquidity: Account<Mint>,
    pub mint_a: Account<Mint>,
    pub mint_b: Account<Mint>,
    /// Pool's token A vault.
    #[account(mut)]
    pub pool_account_a: Account<Token>,
    /// Pool's token B vault.
    #[account(mut)]
    pub pool_account_b: Account<Token>,
    /// Depositor's LP token account.
    #[account(
        mut,
        init(idempotent),
        payer = payer,
        token(mint = mint_liquidity, authority = depositor, token_program = token_program),
    )]
    pub depositor_account_liquidity: Account<Token>,
    /// Depositor's token A account.
    #[account(mut)]
    pub depositor_account_a: Account<Token>,
    /// Depositor's token B account.
    #[account(mut)]
    pub depositor_account_b: Account<Token>,
    #[account(mut)]
    pub payer: Signer,
    pub token_program: Program<TokenProgram>,
    pub system_program: Program<SystemProgram>,
}

/// Integer square root via Newton's method.
fn isqrt(n: u128) -> u64 {
    if n == 0 {
        return 0;
    }
    let mut x = n;
    let mut y = (x + 1) / 2;
    while y < x {
        x = y;
        y = (x + n / x) / 2;
    }
    x as u64
}

#[inline(always)]
pub fn handle_deposit_liquidity(
    accounts: &mut DepositLiquidity,
    amount_a: u64,
    amount_b: u64,
    bumps: &DepositLiquidityBumps,
) -> Result<(), ProgramError> {
    // Clamp to what the depositor actually has.
    let depositor_a = accounts.depositor_account_a.amount();
    let depositor_b = accounts.depositor_account_b.amount();
    let mut amount_a = if amount_a > depositor_a { depositor_a } else { amount_a };
    let mut amount_b = if amount_b > depositor_b { depositor_b } else { amount_b };

    let pool_a_amount = accounts.pool_account_a.amount();
    let pool_b_amount = accounts.pool_account_b.amount();
    let pool_creation = pool_a_amount == 0 && pool_b_amount == 0;

    if !pool_creation {
        // Adjust amounts to maintain the pool ratio.
        if pool_a_amount > pool_b_amount {
            amount_a = (amount_b as u128)
                .checked_mul(pool_a_amount as u128)
                .ok_or(ProgramError::ArithmeticOverflow)?
                .checked_div(pool_b_amount as u128)
                .ok_or(ProgramError::ArithmeticOverflow)? as u64;
        } else {
            amount_b = (amount_a as u128)
                .checked_mul(pool_b_amount as u128)
                .ok_or(ProgramError::ArithmeticOverflow)?
                .checked_div(pool_a_amount as u128)
                .ok_or(ProgramError::ArithmeticOverflow)? as u64;
        }
    }

    // Compute liquidity = sqrt(amount_a * amount_b).
    let product = (amount_a as u128)
        .checked_mul(amount_b as u128)
        .ok_or(ProgramError::ArithmeticOverflow)?;
    let mut liquidity = isqrt(product);

    // Lock minimum liquidity on first deposit.
    if pool_creation {
        if liquidity < crate::MINIMUM_LIQUIDITY {
            return Err(ProgramError::InsufficientFunds);
        }
        liquidity -= crate::MINIMUM_LIQUIDITY;
    }

    // Transfer token A to the pool.
    accounts.token_program
        .transfer(&accounts.depositor_account_a, &accounts.pool_account_a, &accounts.depositor, amount_a)
        .invoke()?;

    // Transfer token B to the pool.
    accounts.token_program
        .transfer(&accounts.depositor_account_b, &accounts.pool_account_b, &accounts.depositor, amount_b)
        .invoke()?;

    // Mint LP tokens to the depositor (signed by pool authority).
    // Seed order matches PoolAuthorityPda: [b"authority", amm, mint_a, mint_b, bump].
    let bump = [bumps.pool_authority];
    let seeds: &[Seed] = &[
        Seed::from(crate::AUTHORITY_SEED),
        Seed::from(accounts.amm.address().as_ref()),
        Seed::from(accounts.mint_a.address().as_ref()),
        Seed::from(accounts.mint_b.address().as_ref()),
        Seed::from(&bump as &[u8]),
    ];

    accounts.token_program
        .mint_to(
            &accounts.mint_liquidity,
            &accounts.depositor_account_liquidity,
            &accounts.pool_authority,
            liquidity,
        )
        .invoke_signed(seeds)?;

    Ok(())
}
