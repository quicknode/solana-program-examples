use {
    crate::{
        state::{Config, PoolConfig},
        ConfigPda, LiquidityMintPda, PoolAuthorityPda, PoolPda,
    },
    quasar_lang::prelude::*,
    quasar_spl::prelude::*,
};

/// Seeds reference the `config`, `mint_a`, and `mint_b` account addresses,
/// which must be provided as separate account inputs.
#[derive(Accounts)]
pub struct DepositLiquidityAccounts {
    #[account(address = ConfigPda::seeds())]
    pub config: Account<Config>,
    #[account(address = PoolPda::seeds(config.address(), mint_a.address(), mint_b.address()))]
    pub pool_config: Account<PoolConfig>,
    /// Pool authority PDA.
    #[account(address = PoolAuthorityPda::seeds(config.address(), mint_a.address(), mint_b.address()))]
    pub pool_authority: UncheckedAccount,
    /// Depositor (must be signer to authorise transfers).
    pub depositor: Signer,
    /// LP mint at the LiquidityMintPda.
    ///
    /// Typed as `InterfaceAccount<Mint>` rather than `Account<Mint>` because
    /// newer quasar-lang requires `T: Discriminator` when combining `address =`
    /// with `Account<T>` (it reads `T::BUMP_OFFSET`). SPL `Mint` doesn't
    /// implement `Discriminator`; `InterfaceAccount` takes the generic
    /// existing-account verifier path that doesn't need it.
    #[account(mut, address = LiquidityMintPda::seeds(config.address(), mint_a.address(), mint_b.address()))]
    pub liquidity_provider_mint: InterfaceAccount<Mint>,
    pub mint_a: Account<Mint>,
    pub mint_b: Account<Mint>,
    /// Pool's token A reserve.
    #[account(mut)]
    pub pool_a: Account<Token>,
    /// Pool's token B reserve.
    #[account(mut)]
    pub pool_b: Account<Token>,
    /// Depositor's LP token account.
    #[account(
        mut,
        init(idempotent),
        payer = payer,
        token(mint = liquidity_provider_mint, authority = depositor, token_program = token_program),
    )]
    pub liquidity_provider_token: Account<Token>,
    /// Depositor's token A account.
    #[account(mut)]
    pub token_a: Account<Token>,
    /// Depositor's token B account.
    #[account(mut)]
    pub token_b: Account<Token>,
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
    accounts: &mut DepositLiquidityAccounts,
    amount_a: u64,
    amount_b: u64,
    bumps: &DepositLiquidityAccountsBumps,
) -> Result<(), ProgramError> {
    // Clamp to what the depositor actually has.
    let depositor_a = accounts.token_a.amount();
    let depositor_b = accounts.token_b.amount();
    let mut amount_a = if amount_a > depositor_a { depositor_a } else { amount_a };
    let mut amount_b = if amount_b > depositor_b { depositor_b } else { amount_b };

    // LP curve runs on *effective* reserves (vault balance minus admin's
    // accumulated fee claim). The admin's owed slice is a fixed obligation,
    // not LP-claimable capital, so it must not affect the deposit ratio.
    let pool_a_amount = accounts.pool_a.amount() - accounts.pool_config.admin_fees_owed_a();
    let pool_b_amount = accounts.pool_b.amount() - accounts.pool_config.admin_fees_owed_b();
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
        .transfer(&accounts.token_a, &accounts.pool_a, &accounts.depositor, amount_a)
        .invoke()?;

    // Transfer token B to the pool.
    accounts.token_program
        .transfer(&accounts.token_b, &accounts.pool_b, &accounts.depositor, amount_b)
        .invoke()?;

    // Mint LP tokens to the depositor (signed by pool authority).
    // Seed order matches PoolAuthorityPda: [b"authority", config, mint_a, mint_b, bump].
    let bump = [bumps.pool_authority];
    let seeds: &[Seed] = &[
        Seed::from(crate::AUTHORITY_SEED),
        Seed::from(accounts.config.address().as_ref()),
        Seed::from(accounts.mint_a.address().as_ref()),
        Seed::from(accounts.mint_b.address().as_ref()),
        Seed::from(&bump as &[u8]),
    ];

    accounts.token_program
        .mint_to(
            &accounts.liquidity_provider_mint,
            &accounts.liquidity_provider_token,
            &accounts.pool_authority,
            liquidity,
        )
        .invoke_signed(seeds)?;

    Ok(())
}
