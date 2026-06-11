use {
    crate::{
        error::AmmError,
        state::{Config, PoolConfig},
        ConfigPda, LiquidityMintPda, PoolAuthorityPda, PoolPda,
    },
    quasar_lang::prelude::*,
    quasar_spl::prelude::*,
};

/// Seeds reference the `config`, `mint_a`, and `mint_b` account addresses,
/// which must be provided as separate account inputs.
#[derive(Accounts)]
pub struct DepositLiquidityAccountConstraints {
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

/// Integer square root via Newton's method. Operates on and returns `u128`;
/// callers narrow with `try_from` so an out-of-range result is a named error
/// instead of a silent truncation.
fn isqrt(n: u128) -> u128 {
    if n == 0 {
        return 0;
    }
    let mut x = n;
    let mut y = (x + 1) / 2;
    while y < x {
        x = y;
        y = (x + n / x) / 2;
    }
    x
}

#[inline(always)]
pub fn handle_deposit_liquidity(
    accounts: &mut DepositLiquidityAccountConstraints,
    amount_a: u64,
    amount_b: u64,
    minimum_lp_tokens_out: u64,
    bumps: &DepositLiquidityAccountConstraintsBumps,
) -> Result<(), ProgramError> {
    // Fail fast if the depositor lacks the requested balance. Never silently
    // clamp to the available balance: callers expect their requested amount to
    // be the amount actually deposited (slippage logic builds on top of it).
    let depositor_a = accounts.token_a.amount();
    let depositor_b = accounts.token_b.amount();
    if amount_a > depositor_a || amount_b > depositor_b {
        return Err(AmmError::InsufficientBalance.into());
    }

    // LP curve runs on *effective* reserves (vault balance minus admin's
    // accumulated fee claim). The admin's owed slice is a fixed obligation,
    // not LP-claimable capital, so it must not affect the deposit ratio.
    // checked_sub: a raw `-` would wrap silently on a BPF release build if the
    // owed slice ever exceeded the vault balance.
    let pool_a_amount = accounts
        .pool_a
        .amount()
        .checked_sub(accounts.pool_config.admin_fees_owed_a())
        .ok_or(AmmError::MathOverflow)?;
    let pool_b_amount = accounts
        .pool_b
        .amount()
        .checked_sub(accounts.pool_config.admin_fees_owed_b())
        .ok_or(AmmError::MathOverflow)?;
    let pool_creation = pool_a_amount == 0 && pool_b_amount == 0;

    // Clamp the caller's (amount_a, amount_b) to the current pool ratio.
    //
    // The caller's amounts are *upper bounds*: at most one side can be used in
    // full, and the other is scaled DOWN to match the current price. This is
    // Uniswap V2's `_addLiquidity` pattern: try the full `amount_a` first and
    // compute the token B it requires; if that fits within the caller's
    // `amount_b`, done - otherwise `amount_b` is the binding side, so use it
    // in full and scale `amount_a` down. Branching on which USER amount is
    // binding (never on reserve sizes) guarantees neither side is ever scaled
    // UP past what the caller offered and the balance check above verified.
    //
    // All ratio math is u128 with checked arithmetic: `amount * reserve` can
    // overflow u64, and the final narrowing uses try_from so an oversized
    // result is a named error, not a truncation.
    let (amount_a, amount_b) = if pool_creation {
        // First deposit sets the initial price; both amounts are used as is.
        (amount_a, amount_b)
    } else {
        // Round down: this can only ask the depositor for *less* of the other
        // token than perfect-ratio, which favours the pool by a sub-minor-unit
        // amount and matches Uniswap V2.
        let amount_b_required = (amount_a as u128)
            .checked_mul(pool_b_amount as u128)
            .ok_or(AmmError::MathOverflow)?
            .checked_div(pool_a_amount as u128)
            .ok_or(AmmError::MathOverflow)?;
        if amount_b_required <= amount_b as u128 {
            // The caller's `amount_b` covers the ratio: use the full
            // `amount_a` and clamp `amount_b` down.
            let amount_b_required =
                u64::try_from(amount_b_required).map_err(|_| AmmError::MathOverflow)?;
            (amount_a, amount_b_required)
        } else {
            // `amount_b` is the binding side: use it in full and clamp
            // `amount_a` down to what the ratio needs.
            let amount_a_required = (amount_b as u128)
                .checked_mul(pool_a_amount as u128)
                .ok_or(AmmError::MathOverflow)?
                .checked_div(pool_b_amount as u128)
                .ok_or(AmmError::MathOverflow)?;
            let amount_a_required =
                u64::try_from(amount_a_required).map_err(|_| AmmError::MathOverflow)?;
            (amount_a_required, amount_b)
        }
    };

    // After clamping, both sides must contribute something. If either side
    // rounds to zero the deposit is too small to register at the current
    // ratio. Fail rather than mint zero-priced LP shares.
    if !pool_creation && (amount_a == 0 || amount_b == 0) {
        return Err(AmmError::DepositAmountTooSmall.into());
    }

    // LP-mint math, two branches:
    //   - First deposit: liquidity = sqrt(a * b) - MINIMUM_LIQUIDITY. The
    //     geometric mean bootstraps the pool; the locked floor is burned
    //     forever to prevent the first depositor draining the pool later.
    //   - Subsequent deposit: liquidity = min(a * supply / pool_a,
    //     b * supply / pool_b), proportional to the depositor's share of each
    //     reserve. The geometric mean must not be used here: it breaks
    //     proportionality once the pool has existing supply.
    let liquidity: u64 = if pool_creation {
        let product = (amount_a as u128)
            .checked_mul(amount_b as u128)
            .ok_or(AmmError::MathOverflow)?;
        let sqrt = u64::try_from(isqrt(product)).map_err(|_| AmmError::MathOverflow)?;
        if sqrt < crate::MINIMUM_LIQUIDITY {
            return Err(AmmError::DepositTooSmall.into());
        }
        sqrt.checked_sub(crate::MINIMUM_LIQUIDITY)
            .ok_or(AmmError::MathOverflow)?
    } else {
        let total_supply = accounts.liquidity_provider_mint.supply() as u128;
        let from_a = (amount_a as u128)
            .checked_mul(total_supply)
            .ok_or(AmmError::MathOverflow)?
            .checked_div(pool_a_amount as u128)
            .ok_or(AmmError::MathOverflow)?;
        let from_b = (amount_b as u128)
            .checked_mul(total_supply)
            .ok_or(AmmError::MathOverflow)?
            .checked_div(pool_b_amount as u128)
            .ok_or(AmmError::MathOverflow)?;
        u64::try_from(from_a.min(from_b)).map_err(|_| AmmError::MathOverflow)?
    };

    // Reject deposits too small to mint any LP tokens (skill: never mint
    // zero-priced shares).
    if liquidity == 0 {
        return Err(AmmError::DepositTooSmall.into());
    }

    // Depositor's slippage protection: the caller passes the lowest LP amount
    // they will accept (computed offchain at quote time). If the pool ratio
    // shifted between quoting and landing, the clamp above used smaller
    // amounts and the LP mint drops; revert rather than mint fewer LP tokens
    // than the caller expects. This is the lower-bound guard; the ratio clamp
    // is the upper-bound guard (caps how much of each token can be spent).
    require!(
        liquidity >= minimum_lp_tokens_out,
        AmmError::DepositBelowMinimum
    );

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
