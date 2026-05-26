use {
    crate::{
        state::{Config, PoolConfig, PoolConfigInner},
        ConfigPda, PoolAuthorityPda, PoolPda,
    },
    quasar_lang::prelude::*,
    quasar_spl::prelude::*,
};

/// Accounts for swapping tokens using the constant-product formula.
///
/// `pool_config` is mutable because each swap accumulates the admin's slice
/// of the trading fee into `admin_fees_owed_a` / `admin_fees_owed_b`.
#[derive(Accounts)]
pub struct SwapTokensAccounts {
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
    accounts: &mut SwapTokensAccounts,
    input_is_token_a: bool,
    input_amount: u64,
    min_output_amount: u64,
    bumps: &SwapTokensAccountsBumps,
) -> Result<(), ProgramError> {
    // Clamp input to what the trader has.
    let input = if input_is_token_a {
        let trader_a = accounts.token_a.amount();
        if input_amount > trader_a { trader_a } else { input_amount }
    } else {
        let trader_b = accounts.token_b.amount();
        if input_amount > trader_b { trader_b } else { input_amount }
    };

    // Split the trading fee between LPs and the admin.
    //   fee_amount    = total fee charged on the input side
    //   admin_portion = admin's slice (accumulates as a virtual claim)
    //   lp_portion    = fee_amount - admin_portion (stays in the reserves,
    //                   boosting LP yield)
    // The admin's slice is *not* transferred immediately; it bumps
    // `pool_config.admin_fees_owed_<input_side>` and is swept later by
    // `claim_admin_fees`. This saves a CPI per swap.
    let fee = accounts.config.fee() as u64;
    let admin_share_bps = accounts.config.admin_share_bps() as u64;
    let fee_amount = input * fee / 10000;
    let admin_portion = fee_amount * admin_share_bps / 10000;
    let taxed_input = input - fee_amount;

    // Effective reserves = raw vault balance - admin's accumulated claim.
    // The constant-product curve runs on the LP-claimable portion only, so
    // the admin's outstanding fees do not contribute to LP yield and do not
    // distort the price.
    let pool_a_raw = accounts.pool_a.amount();
    let pool_b_raw = accounts.pool_b.amount();
    let owed_a = accounts.pool_config.admin_fees_owed_a();
    let owed_b = accounts.pool_config.admin_fees_owed_b();
    let effective_pool_a = pool_a_raw - owed_a;
    let effective_pool_b = pool_b_raw - owed_b;

    let output = if input_is_token_a {
        (taxed_input as u128)
            .checked_mul(effective_pool_b as u128)
            .ok_or(ProgramError::ArithmeticOverflow)?
            .checked_div(
                (effective_pool_a as u128)
                    .checked_add(taxed_input as u128)
                    .ok_or(ProgramError::ArithmeticOverflow)?,
            )
            .ok_or(ProgramError::ArithmeticOverflow)? as u64
    } else {
        (taxed_input as u128)
            .checked_mul(effective_pool_a as u128)
            .ok_or(ProgramError::ArithmeticOverflow)?
            .checked_div(
                (effective_pool_b as u128)
                    .checked_add(taxed_input as u128)
                    .ok_or(ProgramError::ArithmeticOverflow)?,
            )
            .ok_or(ProgramError::ArithmeticOverflow)? as u64
    };

    if output < min_output_amount {
        return Err(ProgramError::Custom(4)); // OutputTooSmall
    }

    // Record invariant on the *effective* reserves before the trade. Using
    // raw balances would let the admin's accumulated fees count toward LP
    // yield (wrong).
    let invariant = (effective_pool_a as u128)
        .checked_mul(effective_pool_b as u128)
        .ok_or(ProgramError::ArithmeticOverflow)?;

    // Build authority signer seeds.
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

    // Accumulate the admin's slice on the *input* side. The fee always
    // comes off the input, so the admin's claim grows in the input token.
    let (new_owed_a, new_owed_b) = if input_is_token_a {
        (
            owed_a.checked_add(admin_portion).ok_or(ProgramError::ArithmeticOverflow)?,
            owed_b,
        )
    } else {
        (
            owed_a,
            owed_b.checked_add(admin_portion).ok_or(ProgramError::ArithmeticOverflow)?,
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

    // Verify invariant holds on the LP-claimable (effective) reserves.
    let new_pool_a_raw = (pool_a_raw as u128)
        + if input_is_token_a { input as u128 } else { 0 }
        - if !input_is_token_a { output as u128 } else { 0 };
    let new_pool_b_raw = (pool_b_raw as u128)
        + if !input_is_token_a { input as u128 } else { 0 }
        - if input_is_token_a { output as u128 } else { 0 };
    let new_effective_a = new_pool_a_raw - (new_owed_a as u128);
    let new_effective_b = new_pool_b_raw - (new_owed_b as u128);
    let new_invariant = new_effective_a
        .checked_mul(new_effective_b)
        .ok_or(ProgramError::ArithmeticOverflow)?;

    if new_invariant < invariant {
        return Err(ProgramError::Custom(5)); // InvariantViolated
    }

    Ok(())
}
