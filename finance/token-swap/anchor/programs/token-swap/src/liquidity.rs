//! The two things every liquidity deposit ends with, shared by
//! `initialize_pool` (the creator's first deposit) and `deposit_liquidity`
//! (every later one): moving both tokens into the reserves and minting LP
//! tokens signed by `pool_config`. Only the arithmetic that decides the
//! amounts differs between the two handlers, and the first deposit's
//! arithmetic lives here too so there is one copy of it.

use anchor_lang::prelude::*;
use anchor_spl::token_interface::{
    self, Mint, MintTo, TokenAccount, TokenInterface, TransferChecked,
};

use crate::{constants::MINIMUM_LIQUIDITY, errors::AmmError, state::PoolConfig};

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
pub fn initial_lp_amount(amount_a: u64, amount_b: u64) -> Result<u64> {
    let product = (amount_a as u128)
        .checked_mul(amount_b as u128)
        .ok_or(AmmError::MathOverflow)?;
    let sqrt_product = u64::try_from(integer_sqrt(product)).map_err(|_| AmmError::MathOverflow)?;
    if sqrt_product <= MINIMUM_LIQUIDITY {
        return err!(AmmError::DepositTooSmall);
    }
    let lp_amount = sqrt_product
        .checked_sub(MINIMUM_LIQUIDITY)
        .ok_or(AmmError::MathOverflow)?;
    Ok(lp_amount)
}

/// The accounts a deposit moves tokens between. `pool_config` owns both
/// reserves and is the LP mint's authority, so it signs the mint with its own
/// seeds `[config, mint_a, mint_b, bump]`.
///
/// The accounts a CPI writes are `&mut`: v2 hands a writable CPI slot a
/// `CpiHandleMut`, which borrows the wrapper mutably. `pool_config` is `&mut`
/// too, because it has to hand back its data borrow while it signs.
pub struct LiquidityDepositAccounts<'a> {
    pub token_program: &'a Interface<'static, TokenInterface>,
    pub pool_config: &'a mut BorshAccount<PoolConfig>,
    pub mint_a: &'a InterfaceAccount<Mint>,
    pub mint_b: &'a InterfaceAccount<Mint>,
    pub pool_a: &'a mut InterfaceAccount<TokenAccount>,
    pub pool_b: &'a mut InterfaceAccount<TokenAccount>,
    pub depositor: &'a Signer,
    pub depositor_token_a: &'a mut InterfaceAccount<TokenAccount>,
    pub depositor_token_b: &'a mut InterfaceAccount<TokenAccount>,
    pub liquidity_provider_mint: &'a mut InterfaceAccount<Mint>,
    pub liquidity_provider_token: &'a mut InterfaceAccount<TokenAccount>,
}

/// Moves `amount_a` and `amount_b` from the depositor's token accounts into
/// the reserves, then mints `lp_amount` LP tokens to the depositor. The
/// caller has already decided all three amounts.
///
/// `transfer_checked` carries the mint and decimals through the CPI, so a
/// wrong-mint or wrong-decimals account fails at the token program instead of
/// moving the wrong quantity.
///
/// `pool_config` signs the mint. It is a data account holding a live borrow
/// on its buffer, which the runtime would reject when the CPI borrows the
/// same account, so this releases the borrow before the mint and leaves it
/// released. A caller that loaded `pool_config` mutably (`initialize_pool`,
/// which creates it) takes the borrow back with `reacquire_borrow_mut` once
/// this returns; a caller that loaded it read-only (`deposit_liquidity`) has
/// nothing to take back.
pub fn deposit_and_mint_lp_tokens(
    accounts: LiquidityDepositAccounts,
    amount_a: u64,
    amount_b: u64,
    lp_amount: u64,
) -> Result<()> {
    token_interface::transfer_checked(
        CpiContext::new(
            accounts.token_program.address(),
            TransferChecked {
                from: accounts.depositor_token_a.to_cpi_handle_mut(),
                mint: accounts.mint_a.to_cpi_handle(),
                to: accounts.pool_a.to_cpi_handle_mut(),
                authority: accounts.depositor.cpi_handle(),
            },
        ),
        amount_a,
        accounts.mint_a.decimals(),
    )?;
    token_interface::transfer_checked(
        CpiContext::new(
            accounts.token_program.address(),
            TransferChecked {
                from: accounts.depositor_token_b.to_cpi_handle_mut(),
                mint: accounts.mint_b.to_cpi_handle(),
                to: accounts.pool_b.to_cpi_handle_mut(),
                authority: accounts.depositor.cpi_handle(),
            },
        ),
        amount_b,
        accounts.mint_b.decimals(),
    )?;

    // Copy the seed inputs out first: `release_borrow` needs `&mut`, and the
    // data cannot be read once it is released.
    let pool_config = accounts.pool_config;
    let config_bytes = pool_config.config.to_bytes();
    let mint_a_bytes = pool_config.mint_a.to_bytes();
    let mint_b_bytes = pool_config.mint_b.to_bytes();
    let pool_config_bump = [pool_config.bump];
    let signer_seeds: &[&[&[u8]]] = &[&[
        config_bytes.as_ref(),
        mint_a_bytes.as_ref(),
        mint_b_bytes.as_ref(),
        &pool_config_bump,
    ]];

    pool_config.release_borrow()?;

    token_interface::mint_to(
        CpiContext::new_with_signer(
            accounts.token_program.address(),
            MintTo {
                mint: accounts.liquidity_provider_mint.to_cpi_handle_mut(),
                to: accounts.liquidity_provider_token.to_cpi_handle_mut(),
                authority: pool_config.to_cpi_handle(),
            },
            signer_seeds,
        ),
        lp_amount,
    )
}
