//! The two things every liquidity deposit ends with, shared by
//! `initialize_pool` (the creator's first deposit) and `deposit_liquidity`
//! (every later one): moving both tokens into the reserves and minting LP
//! tokens signed by `pool_config`. Only the arithmetic that decides the
//! amounts differs between the two handlers, and the first deposit's
//! arithmetic lives here too so there is one copy of it.

use {
    crate::{error::AmmError, state::PoolConfig, MINIMUM_LIQUIDITY},
    quasar_lang::cpi::Seed,
    quasar_lang::prelude::*,
    quasar_spl::prelude::*,
};

/// Integer sqrt via Newton's method. Operates on `u128` so it can handle the
/// product `amount_a * amount_b` (each is a `u64`, product can fill the full
/// `u128`). Floors the result, which matches Uniswap V2's `Math.sqrt` and
/// keeps the first deposit's LP-mint rounding in the pool's favour.
pub fn integer_sqrt(n: u128) -> u128 {
    if n < 2 {
        return n;
    }
    let mut x = n;
    let mut y = x.div_ceil(2);
    while y < x {
        x = y;
        y = (x + n / x) / 2;
    }
    x
}

/// LP tokens minted to the creator for the deposit that opens a pool:
/// `sqrt(amount_a * amount_b) - MINIMUM_LIQUIDITY`. A deposit whose square
/// root is at or below `MINIMUM_LIQUIDITY` fails with `DepositTooSmall`, so
/// the smallest pool that opens leaves its creator at least 1 LP token.
///
/// The `MINIMUM_LIQUIDITY` floor is never minted to anyone. From then on
/// `deposit_liquidity` and `withdraw_liquidity` divide by
/// `lp_supply + MINIMUM_LIQUIDITY`, so the floor counts as supply that nobody
/// holds. Its share of the reserves stays in the pool, claimable by nobody. An
/// attacker who inflates the value behind each share by donating to the vaults
/// inflates the value behind those units too, and loses that part of the
/// donation. (Uniswap V2 instead mints the floor to the zero address; here, if
/// every LP token is burned, the floor's leftover reserves stay behind and the
/// next depositor mints against them through `deposit_liquidity`'s
/// proportional formula.)
///
/// `u128` with checked arithmetic: `amount_a * amount_b` can fill the full
/// `u128`.
pub fn initial_lp_amount(amount_a: u64, amount_b: u64) -> Result<u64, ProgramError> {
    let product = (amount_a as u128)
        .checked_mul(amount_b as u128)
        .ok_or(AmmError::MathOverflow)?;
    let sqrt_product = u64::try_from(integer_sqrt(product)).map_err(|_| AmmError::MathOverflow)?;
    if sqrt_product <= MINIMUM_LIQUIDITY {
        return Err(AmmError::DepositTooSmall.into());
    }
    sqrt_product
        .checked_sub(MINIMUM_LIQUIDITY)
        .ok_or_else(|| AmmError::MathOverflow.into())
}

/// The accounts a deposit moves tokens between. `pool_config` owns both
/// reserves and is the LP mint's authority, so it signs the mint with its own
/// seeds `[config, mint_a, mint_b, bump]`; the bump comes from the handler's
/// `Bumps`, since `PoolConfig` does not store it.
///
/// Generic over the LP mint's wrapper because `initialize_pool` creates it
/// (`Account<Mint>`) and `deposit_liquidity` reads an existing one
/// (`InterfaceAccount<Mint>`); the CPI only needs the account view.
pub struct LiquidityDepositAccounts<'a, LpMint: AsAccountView> {
    pub token_program: &'a Program<TokenProgram>,
    pub pool_config: &'a Account<PoolConfig>,
    pub pool_config_bump: u8,
    pub mint_a: &'a Account<Mint>,
    pub mint_b: &'a Account<Mint>,
    pub pool_a: &'a Account<Token>,
    pub pool_b: &'a Account<Token>,
    pub depositor: &'a Signer,
    pub depositor_token_a: &'a Account<Token>,
    pub depositor_token_b: &'a Account<Token>,
    pub liquidity_provider_mint: &'a LpMint,
    pub liquidity_provider_token: &'a Account<Token>,
}

/// Moves `amount_a` and `amount_b` from the depositor's token accounts into
/// the reserves, then mints `lp_amount` LP tokens to the depositor. The
/// caller has already decided all three amounts.
///
/// `transfer_checked` carries the mint and decimals through the CPI, so a
/// wrong-mint or wrong-decimals account fails at the token program instead of
/// moving the wrong quantity.
#[inline(always)]
pub fn deposit_and_mint_lp_tokens<LpMint: AsAccountView>(
    accounts: LiquidityDepositAccounts<'_, LpMint>,
    amount_a: u64,
    amount_b: u64,
    lp_amount: u64,
) -> Result<(), ProgramError> {
    accounts
        .token_program
        .transfer_checked(
            accounts.depositor_token_a,
            accounts.mint_a,
            accounts.pool_a,
            accounts.depositor,
            amount_a,
            accounts.mint_a.decimals(),
        )
        .invoke()?;
    accounts
        .token_program
        .transfer_checked(
            accounts.depositor_token_b,
            accounts.mint_b,
            accounts.pool_b,
            accounts.depositor,
            amount_b,
            accounts.mint_b.decimals(),
        )
        .invoke()?;

    // Seed order matches PoolPda: [config, mint_a, mint_b, bump], read from
    // the pool's own record rather than from the mint accounts handed in.
    let pool_config = accounts.pool_config;
    let bump = [accounts.pool_config_bump];
    let seeds: &[Seed] = &[
        Seed::from(pool_config.config().as_ref()),
        Seed::from(pool_config.mint_a().as_ref()),
        Seed::from(pool_config.mint_b().as_ref()),
        Seed::from(&bump as &[u8]),
    ];
    accounts
        .token_program
        .mint_to(
            accounts.liquidity_provider_mint,
            accounts.liquidity_provider_token,
            pool_config,
            lp_amount,
        )
        .invoke_signed(seeds)
}
