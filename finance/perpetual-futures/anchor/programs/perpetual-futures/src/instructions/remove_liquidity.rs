use anchor_lang::prelude::*;
use anchor_spl::{
    associated_token::AssociatedToken,
    token_interface::{
        burn, transfer_checked, Burn, Mint, TokenAccount, TokenInterface, TransferChecked,
    },
};

use crate::constants::{MINIMUM_LIQUIDITY, POOL_SEED, VAULT_SEED};
use crate::errors::PerpError;
use crate::instructions::shared::{
    liquidity_provider_aum, refresh_price_and_funding_within_band, Rounding,
};
use crate::state::Pool;

pub fn handle_remove_liquidity(
    context: &mut Context<RemoveLiquidityAccountConstraints>,
    shares: u64,
    minimum_amount_out: u64,
) -> Result<()> {
    require!(shares > 0, PerpError::ZeroAmount);

    let pool = &mut context.accounts.pool;
    let price = refresh_price_and_funding_within_band(pool, &context.accounts.oracle_feed)?;

    let lp_supply = context.accounts.lp_mint.supply();
    // The pool is valued rounding down, so a fraction of a base unit in the
    // traders' marked profit/loss lowers what a share redeems for.
    let aum = liquidity_provider_aum(pool, price, Rounding::Down)?;
    require!(aum > 0, PerpError::PoolInsolvent);

    // amount_out = shares * assets-under-management / (supply + MINIMUM_LIQUIDITY),
    // floored. The withheld minimum counts as shares nobody holds, as it does
    // in add_liquidity, so its slice of the pool never leaves.
    let total_shares = (lp_supply as u128)
        .checked_add(MINIMUM_LIQUIDITY as u128)
        .ok_or(PerpError::MathOverflow)?;
    let amount_out: u64 = (shares as u128)
        .checked_mul(aum as u128)
        .ok_or(PerpError::MathOverflow)?
        .checked_div(total_shares)
        .ok_or(PerpError::MathOverflow)?
        .try_into()
        .map_err(|_| PerpError::MathOverflow)?;

    require!(amount_out > 0, PerpError::AmountRoundsToZero);
    // Shares are priced against assets-under-management, which counts traders'
    // unrealized losses as the providers' gain. Those losses are still in the
    // traders' collateral until their positions close, so a withdrawal is
    // capped at `liquidity`, the tokens the providers own now. While traders
    // are up instead, the pricing already keeps a withdrawal below `liquidity`
    // minus their profit, leaving that profit's backing in the pool.
    require!(
        amount_out <= pool.liquidity,
        PerpError::InsufficientLiquidity
    );
    require!(
        amount_out >= minimum_amount_out,
        PerpError::SlippageExceeded
    );

    pool.liquidity = pool
        .liquidity
        .checked_sub(amount_out)
        .ok_or(PerpError::MathOverflow)?;

    burn(
        CpiContext::new(
            context.accounts.token_program.address(),
            Burn {
                mint: context.accounts.lp_mint.to_cpi_handle_mut(),
                from: context.accounts.provider_lp.to_cpi_handle_mut(),
                authority: context.accounts.provider.cpi_handle(),
            },
        ),
        shares,
    )?;

    // The pool signs the CPI below with its own seeds. Copy them out first: a
    // data account holds a live borrow on its buffer, which the runtime
    // rejects when the CPI borrows the same account, so the borrow is
    // released around the CPI and taken back after. Releasing commits the
    // writes above; reacquiring re-reads the account.
    let collateral_mint_key = pool.collateral_mint;
    let oracle_feed_key = pool.oracle_feed;
    let pool_bump = [pool.bump];
    let pool_seeds: &[&[u8]] = &[
        POOL_SEED,
        collateral_mint_key.as_ref(),
        oracle_feed_key.as_ref(),
        &pool_bump,
    ];
    context.accounts.pool.release_borrow()?;
    transfer_checked(
        CpiContext::new_with_signer(
            context.accounts.token_program.address(),
            TransferChecked {
                from: context.accounts.custody_vault.to_cpi_handle_mut(),
                mint: context.accounts.collateral_mint.to_cpi_handle(),
                to: context.accounts.provider_collateral.to_cpi_handle_mut(),
                authority: context.accounts.pool.to_cpi_handle(),
            },
            &[pool_seeds],
        ),
        amount_out,
        context.accounts.collateral_mint.decimals(),
    )?;
    context.accounts.pool.reacquire_borrow_mut()?;

    Ok(())
}

#[derive(Accounts)]
pub struct RemoveLiquidityAccountConstraints {
    #[account(mut)]
    pub provider: Signer,

    #[account(
        mut,
        seeds = [POOL_SEED, pool.collateral_mint.as_ref(), pool.oracle_feed.as_ref()],
        bump = pool.bump,
    )]
    pub pool: Box<BorshAccount<Pool>>,

    /// CHECK: validated by the `address = pool.oracle_feed` constraint below.
    #[account(address = pool.oracle_feed)]
    pub oracle_feed: UncheckedAccount,

    #[account(address = pool.collateral_mint)]
    pub collateral_mint: Box<InterfaceAccount<Mint>>,

    #[account(mut, address = pool.lp_mint)]
    pub lp_mint: Box<InterfaceAccount<Mint>>,

    #[account(
        mut,
        seeds = [VAULT_SEED, pool.address().as_ref()],
        bump,
        address = pool.custody_vault,
    )]
    pub custody_vault: Box<InterfaceAccount<TokenAccount>>,

    #[account(
        mut,
        associated_token::mint = collateral_mint,
        associated_token::authority = provider,
        associated_token::token_program = token_program,
    )]
    pub provider_collateral: Box<InterfaceAccount<TokenAccount>>,

    #[account(
        mut,
        associated_token::mint = lp_mint,
        associated_token::authority = provider,
        associated_token::token_program = token_program,
    )]
    pub provider_lp: Box<InterfaceAccount<TokenAccount>>,

    pub token_program: Interface<'static, TokenInterface>,
    pub associated_token_program: Program<AssociatedToken>,
    pub system_program: Program<System>,
}
