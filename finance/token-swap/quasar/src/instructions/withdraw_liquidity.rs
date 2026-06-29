use {
    crate::{
        error::AmmError,
        state::{Config, PoolConfig},
        ConfigPda, LiquidityMintPda, PoolAuthorityPda, PoolPda,
    },
    quasar_lang::prelude::*,
    quasar_spl::prelude::*,
};

#[derive(Accounts)]
pub struct WithdrawLiquidityAccountConstraints {
    #[account(address = ConfigPda::seeds())]
    pub config: Account<Config>,
    #[account(address = PoolPda::seeds(config.address(), mint_a.address(), mint_b.address()))]
    pub pool_config: Account<PoolConfig>,
    /// Pool authority PDA.
    #[account(address = PoolAuthorityPda::seeds(config.address(), mint_a.address(), mint_b.address()))]
    pub pool_authority: UncheckedAccount,
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
    #[account(mut)]
    pub mint_a: Account<Mint>,
    #[account(mut)]
    pub mint_b: Account<Mint>,
    #[account(mut)]
    pub pool_a: Account<Token>,
    #[account(mut)]
    pub pool_b: Account<Token>,
    #[account(mut)]
    pub liquidity_provider_token: Account<Token>,
    #[account(
        mut,
        init(idempotent),
        payer = payer,
        token(mint = mint_a, authority = depositor, token_program = token_program),
    )]
    pub token_a: Account<Token>,
    #[account(
        mut,
        init(idempotent),
        payer = payer,
        token(mint = mint_b, authority = depositor, token_program = token_program),
    )]
    pub token_b: Account<Token>,
    #[account(mut)]
    pub payer: Signer,
    pub token_program: Program<TokenProgram>,
    pub system_program: Program<SystemProgram>,
}

#[inline(always)]
pub fn handle_withdraw_liquidity(
    accounts: &mut WithdrawLiquidityAccountConstraints,
    amount: u64,
    minimum_token_a_out: u64,
    minimum_token_b_out: u64,
    bumps: &WithdrawLiquidityAccountConstraintsBumps,
) -> Result<(), ProgramError> {
    // Seed order matches PoolAuthorityPda: [b"authority", config, mint_a, mint_b, bump].
    let bump = [bumps.pool_authority];
    let seeds: &[Seed] = &[
        Seed::from(crate::AUTHORITY_SEED),
        Seed::from(accounts.config.address().as_ref()),
        Seed::from(accounts.mint_a.address().as_ref()),
        Seed::from(accounts.mint_b.address().as_ref()),
        Seed::from(&bump as &[u8]),
    ];

    // Compute proportional amounts. LPs withdraw a share of the *effective*
    // reserves (vault balance minus the admin's accumulated fee claim).
    // The admin's owed slice physically stays in the vaults but is not
    // distributed to exiting LPs - it's swept separately via
    // `claim_admin_fees`.
    // checked_sub: admin_fees_owed is an invariant subset of the vault balance;
    // a raw `-` would wrap silently on a BPF release build if that ever broke.
    let effective_pool_a = accounts
        .pool_a
        .amount()
        .checked_sub(accounts.pool_config.admin_fees_owed_a())
        .ok_or(AmmError::MathOverflow)?;
    let effective_pool_b = accounts
        .pool_b
        .amount()
        .checked_sub(accounts.pool_config.admin_fees_owed_b())
        .ok_or(AmmError::MathOverflow)?;
    let total_liquidity = accounts
        .liquidity_provider_mint
        .supply()
        .checked_add(crate::MINIMUM_LIQUIDITY)
        .ok_or(AmmError::MathOverflow)?;

    let amount_a_u128 = (amount as u128)
        .checked_mul(effective_pool_a as u128)
        .ok_or(AmmError::MathOverflow)?
        .checked_div(total_liquidity as u128)
        .ok_or(AmmError::MathOverflow)?;
    let amount_a = u64::try_from(amount_a_u128).map_err(|_| AmmError::MathOverflow)?;

    let amount_b_u128 = (amount as u128)
        .checked_mul(effective_pool_b as u128)
        .ok_or(AmmError::MathOverflow)?
        .checked_div(total_liquidity as u128)
        .ok_or(AmmError::MathOverflow)?;
    let amount_b = u64::try_from(amount_b_u128).map_err(|_| AmmError::MathOverflow)?;

    // LP's slippage protection: if the pool ratio shifted between the LP
    // quoting their exit and this transaction landing (e.g. a big swap
    // drained one side), the proportional share comes back with a different
    // mix than expected. Revert so the LP can requote.
    require!(
        amount_a >= minimum_token_a_out,
        AmmError::WithdrawalBelowMinimum
    );
    require!(
        amount_b >= minimum_token_b_out,
        AmmError::WithdrawalBelowMinimum
    );

    // Transfer token A from pool to depositor.
    accounts.token_program
        .transfer(&accounts.pool_a, &accounts.token_a, &accounts.pool_authority, amount_a)
        .invoke_signed(seeds)?;

    // Transfer token B from pool to depositor.
    accounts.token_program
        .transfer(&accounts.pool_b, &accounts.token_b, &accounts.pool_authority, amount_b)
        .invoke_signed(seeds)?;

    // Burn LP tokens.
    accounts.token_program
        .burn(&accounts.liquidity_provider_token, &accounts.liquidity_provider_mint, &accounts.depositor, amount)
        .invoke()?;

    Ok(())
}
