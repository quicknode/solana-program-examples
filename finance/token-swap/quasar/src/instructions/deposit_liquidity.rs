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
    // Fail fast if the depositor lacks the requested balance. Never silently
    // clamp to the available balance: callers expect their requested amount to
    // be the amount actually deposited (slippage logic builds on top of it).
    let depositor_a = accounts.token_a.amount();
    let depositor_b = accounts.token_b.amount();
    if amount_a > depositor_a || amount_b > depositor_b {
        return Err(ProgramError::InsufficientFunds);
    }
    let mut amount_a = amount_a;
    let mut amount_b = amount_b;

    // LP curve runs on *effective* reserves (vault balance minus admin's
    // accumulated fee claim). The admin's owed slice is a fixed obligation,
    // not LP-claimable capital, so it must not affect the deposit ratio.
    // checked_sub: a raw `-` would wrap silently on a BPF release build if the
    // owed slice ever exceeded the vault balance.
    let pool_a_amount = accounts
        .pool_a
        .amount()
        .checked_sub(accounts.pool_config.admin_fees_owed_a())
        .ok_or(ProgramError::ArithmeticOverflow)?;
    let pool_b_amount = accounts
        .pool_b
        .amount()
        .checked_sub(accounts.pool_config.admin_fees_owed_b())
        .ok_or(ProgramError::ArithmeticOverflow)?;
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

    // LP-mint math, two branches:
    //   - First deposit: liquidity = sqrt(a * b) - MINIMUM_LIQUIDITY. The
    //     geometric mean bootstraps the pool; the locked floor is burned
    //     forever to prevent the first depositor draining the pool later.
    //   - Subsequent deposit: liquidity = min(a * supply / pool_a,
    //     b * supply / pool_b), proportional to the depositor's share of each
    //     reserve. Using sqrt(a * b) for *every* deposit (the previous
    //     behaviour) breaks proportionality on subsequent deposits.
    let liquidity: u64 = if pool_creation {
        let product = (amount_a as u128)
            .checked_mul(amount_b as u128)
            .ok_or(ProgramError::ArithmeticOverflow)?;
        let sqrt = isqrt(product);
        if sqrt < crate::MINIMUM_LIQUIDITY {
            return Err(ProgramError::InsufficientFunds);
        }
        sqrt.checked_sub(crate::MINIMUM_LIQUIDITY)
            .ok_or(ProgramError::ArithmeticOverflow)?
    } else {
        let total_supply = accounts.liquidity_provider_mint.supply() as u128;
        let from_a = (amount_a as u128)
            .checked_mul(total_supply)
            .ok_or(ProgramError::ArithmeticOverflow)?
            .checked_div(pool_a_amount as u128)
            .ok_or(ProgramError::ArithmeticOverflow)?;
        let from_b = (amount_b as u128)
            .checked_mul(total_supply)
            .ok_or(ProgramError::ArithmeticOverflow)?
            .checked_div(pool_b_amount as u128)
            .ok_or(ProgramError::ArithmeticOverflow)?;
        u64::try_from(from_a.min(from_b)).map_err(|_| ProgramError::ArithmeticOverflow)?
    };

    // Reject deposits too small to mint any LP tokens (skill: never mint
    // zero-priced shares).
    if liquidity == 0 {
        return Err(ProgramError::InsufficientFunds);
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
