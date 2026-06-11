use {
    crate::{
        error::AmmError,
        state::{Config, PoolConfig, PoolConfigInner},
        ConfigPda, PoolAuthorityPda, PoolPda, BASIS_POINTS_DIVISOR,
    },
    quasar_lang::prelude::*,
    quasar_spl::prelude::*,
};

/// `pool_config` is mutable because each swap accumulates the admin's slice
/// of the trading fee into `admin_fees_owed_a` / `admin_fees_owed_b`.
#[derive(Accounts)]
pub struct SwapTokensAccountConstraints {
    #[account(address = ConfigPda::seeds())]
    pub config: Account<Config>,
    #[account(
        mut,
        address = PoolPda::seeds(config.address(), mint_a.address(), mint_b.address()),
    )]
    pub pool_config: Account<PoolConfig>,
    /// Pool authority PDA.
    #[account(address = PoolAuthorityPda::seeds(config.address(), mint_a.address(), mint_b.address()))]
    pub pool_authority: UncheckedAccount,
    pub trader: Signer,
    pub mint_a: Account<Mint>,
    pub mint_b: Account<Mint>,
    #[account(mut)]
    pub pool_a: Account<Token>,
    #[account(mut)]
    pub pool_b: Account<Token>,
    #[account(
        mut,
        init(idempotent),
        payer = payer,
        token(mint = mint_a, authority = trader, token_program = token_program),
    )]
    pub token_a: Account<Token>,
    #[account(
        mut,
        init(idempotent),
        payer = payer,
        token(mint = mint_b, authority = trader, token_program = token_program),
    )]
    pub token_b: Account<Token>,
    #[account(mut)]
    pub payer: Signer,
    pub token_program: Program<TokenProgram>,
    pub system_program: Program<SystemProgram>,
}

#[inline(always)]
pub fn handle_swap_tokens(
    accounts: &mut SwapTokensAccountConstraints,
    input_is_token_a: bool,
    input_amount: u64,
    min_output_amount: u64,
    bumps: &SwapTokensAccountConstraintsBumps,
) -> Result<(), ProgramError> {
    // Never silently clamp the input to the trader's balance: the trader's
    // min_output_amount is computed against the input they requested, so
    // clamping would let the swap fill at terms they never agreed to. Fail
    // fast instead and let the client re-quote.
    let trader_balance = if input_is_token_a {
        accounts.token_a.amount()
    } else {
        accounts.token_b.amount()
    };
    if input_amount > trader_balance {
        return Err(AmmError::InsufficientBalance.into());
    }
    let input = input_amount;

    // Split the trading fee between LPs and the admin.
    //   fee_amount    = total fee charged on the input side
    //   admin_portion = admin's slice (accumulates as a virtual claim)
    //   lp_portion    = fee_amount - admin_portion (stays in the reserves,
    //                   boosting LP yield)
    // The admin's slice is *not* transferred immediately; it bumps
    // `pool_config.admin_fees_owed_<input_side>` and is swept later by
    // `claim_admin_fees`. This saves a CPI per swap. u128 + checked: the
    // intermediate `input * fee` can overflow u64; multiply before divide.
    let fee = accounts.config.fee() as u128;
    let admin_share_bps = accounts.config.admin_share_bps() as u128;
    let fee_amount_u128 = (input as u128)
        .checked_mul(fee)
        .ok_or(AmmError::MathOverflow)?
        .checked_div(BASIS_POINTS_DIVISOR as u128)
        .ok_or(AmmError::MathOverflow)?;
    let fee_amount = u64::try_from(fee_amount_u128).map_err(|_| AmmError::MathOverflow)?;
    let admin_portion_u128 = (fee_amount as u128)
        .checked_mul(admin_share_bps)
        .ok_or(AmmError::MathOverflow)?
        .checked_div(BASIS_POINTS_DIVISOR as u128)
        .ok_or(AmmError::MathOverflow)?;
    let admin_portion = u64::try_from(admin_portion_u128).map_err(|_| AmmError::MathOverflow)?;
    let taxed_input = input
        .checked_sub(fee_amount)
        .ok_or(AmmError::MathOverflow)?;

    // Effective reserves = raw vault balance - admin's accumulated claim.
    // The constant-product curve runs on the LP-claimable portion only, so
    // the admin's outstanding fees do not contribute to LP yield and do not
    // distort the price. checked_sub: a raw `-` would wrap silently on a BPF
    // release build if the owed slice ever exceeded the vault balance.
    let pool_a_raw = accounts.pool_a.amount();
    let pool_b_raw = accounts.pool_b.amount();
    let owed_a = accounts.pool_config.admin_fees_owed_a();
    let owed_b = accounts.pool_config.admin_fees_owed_b();
    let effective_pool_a = pool_a_raw
        .checked_sub(owed_a)
        .ok_or(AmmError::MathOverflow)?;
    let effective_pool_b = pool_b_raw
        .checked_sub(owed_b)
        .ok_or(AmmError::MathOverflow)?;

    let output_u128 = if input_is_token_a {
        (taxed_input as u128)
            .checked_mul(effective_pool_b as u128)
            .ok_or(AmmError::MathOverflow)?
            .checked_div(
                (effective_pool_a as u128)
                    .checked_add(taxed_input as u128)
                    .ok_or(AmmError::MathOverflow)?,
            )
            .ok_or(AmmError::MathOverflow)?
    } else {
        (taxed_input as u128)
            .checked_mul(effective_pool_a as u128)
            .ok_or(AmmError::MathOverflow)?
            .checked_div(
                (effective_pool_b as u128)
                    .checked_add(taxed_input as u128)
                    .ok_or(AmmError::MathOverflow)?,
            )
            .ok_or(AmmError::MathOverflow)?
    };
    let output = u64::try_from(output_u128).map_err(|_| AmmError::MathOverflow)?;

    // Trader's slippage protection: revert if the pool moved between quote
    // and landing and the output dropped below the trader's floor.
    require!(output >= min_output_amount, AmmError::SlippageExceeded);

    // Record invariant on the *effective* reserves before the trade. Using
    // raw balances would let the admin's accumulated fees count toward LP
    // yield (wrong).
    let invariant = (effective_pool_a as u128)
        .checked_mul(effective_pool_b as u128)
        .ok_or(AmmError::MathOverflow)?;

    // Effects (Checks-Effects-Interactions): accumulate the admin's slice on
    // the *input* side before any transfer CPI. The fee always comes off the
    // input, so the admin's claim grows in the input token. Writing state
    // before the interactions is the safe ordering - a failed CPI reverts the
    // whole transaction, so the accumulator update can never outlive a failed
    // transfer.
    let (new_owed_a, new_owed_b) = if input_is_token_a {
        (
            owed_a.checked_add(admin_portion).ok_or(AmmError::MathOverflow)?,
            owed_b,
        )
    } else {
        (
            owed_a,
            owed_b.checked_add(admin_portion).ok_or(AmmError::MathOverflow)?,
        )
    };
    let config_addr = *accounts.pool_config.config();
    let mint_a_addr = *accounts.pool_config.mint_a();
    let mint_b_addr = *accounts.pool_config.mint_b();
    accounts.pool_config.set_inner(PoolConfigInner {
        config: config_addr,
        mint_a: mint_a_addr,
        mint_b: mint_b_addr,
        admin_fees_owed_a: new_owed_a,
        admin_fees_owed_b: new_owed_b,
    });

    // Interactions: the token transfers, after state is written.
    // Seed order matches PoolAuthorityPda: [b"authority", config, mint_a, mint_b, bump].
    let bump = [bumps.pool_authority];
    let seeds: &[Seed] = &[
        Seed::from(crate::AUTHORITY_SEED),
        Seed::from(accounts.config.address().as_ref()),
        Seed::from(accounts.mint_a.address().as_ref()),
        Seed::from(accounts.mint_b.address().as_ref()),
        Seed::from(&bump as &[u8]),
    ];

    if input_is_token_a {
        // Trader sends token A to pool.
        accounts.token_program
            .transfer(&accounts.token_a, &accounts.pool_a, &accounts.trader, input)
            .invoke()?;
        // Pool sends token B to trader (signed).
        accounts.token_program
            .transfer(&accounts.pool_b, &accounts.token_b, &accounts.pool_authority, output)
            .invoke_signed(seeds)?;
    } else {
        // Pool sends token A to trader (signed).
        accounts.token_program
            .transfer(&accounts.pool_a, &accounts.token_a, &accounts.pool_authority, output)
            .invoke_signed(seeds)?;
        // Trader sends token B to pool.
        accounts.token_program
            .transfer(&accounts.token_b, &accounts.pool_b, &accounts.trader, input)
            .invoke()?;
    }

    // Verify invariant holds on the LP-claimable (effective) reserves.
    // u128 + checked throughout - a raw `+`/`-` could wrap on extreme values.
    let new_pool_a_raw = (pool_a_raw as u128)
        .checked_add(if input_is_token_a { input as u128 } else { 0 })
        .ok_or(AmmError::MathOverflow)?
        .checked_sub(if !input_is_token_a { output as u128 } else { 0 })
        .ok_or(AmmError::MathOverflow)?;
    let new_pool_b_raw = (pool_b_raw as u128)
        .checked_add(if !input_is_token_a { input as u128 } else { 0 })
        .ok_or(AmmError::MathOverflow)?
        .checked_sub(if input_is_token_a { output as u128 } else { 0 })
        .ok_or(AmmError::MathOverflow)?;
    let new_effective_a = new_pool_a_raw
        .checked_sub(new_owed_a as u128)
        .ok_or(AmmError::MathOverflow)?;
    let new_effective_b = new_pool_b_raw
        .checked_sub(new_owed_b as u128)
        .ok_or(AmmError::MathOverflow)?;
    let new_invariant = new_effective_a
        .checked_mul(new_effective_b)
        .ok_or(AmmError::MathOverflow)?;

    require!(new_invariant >= invariant, AmmError::InvariantViolated);

    Ok(())
}
