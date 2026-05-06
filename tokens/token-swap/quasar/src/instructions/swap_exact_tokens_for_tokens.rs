use {
    crate::{
        state::{Amm, Pool},
        AmmPda, PoolAuthorityPda, PoolPda,
    },
    quasar_lang::prelude::*,
    quasar_spl::prelude::*,
};

/// Accounts for swapping tokens using the constant-product formula.
#[derive(Accounts)]
pub struct SwapExactTokensForTokens {
    #[account(address = AmmPda::seeds())]
    pub amm: Account<Amm>,
    #[account(address = PoolPda::seeds(amm.address(), mint_a.address(), mint_b.address()))]
    pub pool: Account<Pool>,
    /// Pool authority PDA.
    #[account(address = PoolAuthorityPda::seeds(amm.address(), mint_a.address(), mint_b.address()))]
    pub pool_authority: UncheckedAccount,
    pub trader: Signer,
    pub mint_a: Account<Mint>,
    pub mint_b: Account<Mint>,
    #[account(mut)]
    pub pool_account_a: Account<Token>,
    #[account(mut)]
    pub pool_account_b: Account<Token>,
    #[account(
        mut,
        init(idempotent),
        payer = payer,
        token(mint = mint_a, authority = trader, token_program = token_program),
    )]
    pub trader_account_a: Account<Token>,
    #[account(
        mut,
        init(idempotent),
        payer = payer,
        token(mint = mint_b, authority = trader, token_program = token_program),
    )]
    pub trader_account_b: Account<Token>,
    #[account(mut)]
    pub payer: Signer,
    pub token_program: Program<TokenProgram>,
    pub system_program: Program<SystemProgram>,
}

#[inline(always)]
pub fn handle_swap_exact_tokens_for_tokens(
    accounts: &mut SwapExactTokensForTokens,
    swap_a: bool,
    input_amount: u64,
    min_output_amount: u64,
    bumps: &SwapExactTokensForTokensBumps,
) -> Result<(), ProgramError> {
    // Clamp input to what the trader has.
    let input = if swap_a {
        let trader_a = accounts.trader_account_a.amount();
        if input_amount > trader_a { trader_a } else { input_amount }
    } else {
        let trader_b = accounts.trader_account_b.amount();
        if input_amount > trader_b { trader_b } else { input_amount }
    };

    // Apply fee.
    let fee = accounts.amm.fee.get() as u64;
    let taxed_input = input - input * fee / 10000;

    // Constant-product formula: output = taxed_input * pool_out / (pool_in + taxed_input)
    let pool_a = accounts.pool_account_a.amount();
    let pool_b = accounts.pool_account_b.amount();

    let output = if swap_a {
        (taxed_input as u128)
            .checked_mul(pool_b as u128)
            .ok_or(ProgramError::ArithmeticOverflow)?
            .checked_div(
                (pool_a as u128)
                    .checked_add(taxed_input as u128)
                    .ok_or(ProgramError::ArithmeticOverflow)?,
            )
            .ok_or(ProgramError::ArithmeticOverflow)? as u64
    } else {
        (taxed_input as u128)
            .checked_mul(pool_a as u128)
            .ok_or(ProgramError::ArithmeticOverflow)?
            .checked_div(
                (pool_b as u128)
                    .checked_add(taxed_input as u128)
                    .ok_or(ProgramError::ArithmeticOverflow)?,
            )
            .ok_or(ProgramError::ArithmeticOverflow)? as u64
    };

    if output < min_output_amount {
        return Err(ProgramError::Custom(4)); // OutputTooSmall
    }

    // Record invariant before the trade.
    let invariant = (pool_a as u128)
        .checked_mul(pool_b as u128)
        .ok_or(ProgramError::ArithmeticOverflow)?;

    // Build authority signer seeds.
    // Seed order matches PoolAuthorityPda: [b"authority", amm, mint_a, mint_b, bump].
    let bump = [bumps.pool_authority];
    let seeds: &[Seed] = &[
        Seed::from(crate::AUTHORITY_SEED),
        Seed::from(accounts.amm.address().as_ref()),
        Seed::from(accounts.mint_a.address().as_ref()),
        Seed::from(accounts.mint_b.address().as_ref()),
        Seed::from(&bump as &[u8]),
    ];

    if swap_a {
        // Trader sends token A to pool.
        accounts.token_program
            .transfer(&accounts.trader_account_a, &accounts.pool_account_a, &accounts.trader, input)
            .invoke()?;
        // Pool sends token B to trader (signed).
        accounts.token_program
            .transfer(&accounts.pool_account_b, &accounts.trader_account_b, &accounts.pool_authority, output)
            .invoke_signed(seeds)?;
    } else {
        // Pool sends token A to trader (signed).
        accounts.token_program
            .transfer(&accounts.pool_account_a, &accounts.trader_account_a, &accounts.pool_authority, output)
            .invoke_signed(seeds)?;
        // Trader sends token B to pool.
        accounts.token_program
            .transfer(&accounts.trader_account_b, &accounts.pool_account_b, &accounts.trader, input)
            .invoke()?;
    }

    // Verify invariant holds (new product >= old product).
    let new_pool_a = pool_a as u128
        + if swap_a { input as u128 } else { 0 }
        - if !swap_a { output as u128 } else { 0 };
    let new_pool_b = pool_b as u128
        + if !swap_a { input as u128 } else { 0 }
        - if swap_a { output as u128 } else { 0 };
    let new_invariant = new_pool_a
        .checked_mul(new_pool_b)
        .ok_or(ProgramError::ArithmeticOverflow)?;

    if new_invariant < invariant {
        return Err(ProgramError::Custom(5)); // InvariantViolated
    }

    Ok(())
}
