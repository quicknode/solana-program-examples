use anchor_lang::prelude::*;
use anchor_spl::{
    associated_token::AssociatedToken,
    token_interface::{
        burn, transfer_checked, Burn, Mint, TokenAccount, TokenInterface, TransferChecked,
    },
};

use crate::constants::{AUTHORITY_SEED, POOL_SEED, VAULT_SEED};
use crate::errors::PerpError;
use crate::instructions::shared::{
    liquidity_provider_aum, pool_profit_liability, refresh_price_and_funding,
};
use crate::state::Pool;

pub fn handle_remove_liquidity(
    context: Context<RemoveLiquidityAccountConstraints>,
    shares: u64,
    minimum_amount_out: u64,
) -> Result<()> {
    require!(shares > 0, PerpError::ZeroAmount);

    let pool = &mut context.accounts.pool;
    let price = refresh_price_and_funding(pool, &context.accounts.oracle_feed)?;

    let lp_supply = context.accounts.lp_mint.supply;
    let aum = liquidity_provider_aum(pool, price)?;
    require!(aum > 0, PerpError::PoolInsolvent);

    // amount_out = shares * assets-under-management / supply, floored.
    let amount_out: u64 = (shares as u128)
        .checked_mul(aum as u128)
        .ok_or(PerpError::MathOverflow)?
        .checked_div(lp_supply as u128)
        .ok_or(PerpError::MathOverflow)?
        .try_into()
        .map_err(|_| PerpError::MathOverflow)?;

    require!(amount_out > 0, PerpError::AmountRoundsToZero);
    // Only free liquidity can leave: the backing for the profit traders are
    // currently owed stays put, so providers cannot withdraw out from under a
    // winning trader and force their haircut. Profit traders are owed beyond
    // what liquidity can cover already has `h < 1`, leaving no free liquidity.
    let liability = pool_profit_liability(pool, price)?;
    let free_liquidity = (pool.liquidity as u128).saturating_sub(liability);
    require!(
        amount_out as u128 <= free_liquidity,
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
            context.accounts.token_program.key(),
            Burn {
                mint: context.accounts.lp_mint.to_account_info(),
                from: context.accounts.provider_lp.to_account_info(),
                authority: context.accounts.provider.to_account_info(),
            },
        ),
        shares,
    )?;

    let pool_key = pool.key();
    let authority_seeds: &[&[u8]] = &[AUTHORITY_SEED, pool_key.as_ref(), &[pool.authority_bump]];
    transfer_checked(
        CpiContext::new_with_signer(
            context.accounts.token_program.key(),
            TransferChecked {
                from: context.accounts.custody_vault.to_account_info(),
                mint: context.accounts.collateral_mint.to_account_info(),
                to: context.accounts.provider_collateral.to_account_info(),
                authority: context.accounts.pool_authority.to_account_info(),
            },
            &[authority_seeds],
        ),
        amount_out,
        context.accounts.collateral_mint.decimals,
    )?;

    Ok(())
}

#[derive(Accounts)]
pub struct RemoveLiquidityAccountConstraints<'info> {
    #[account(mut)]
    pub provider: Signer<'info>,

    #[account(
        mut,
        seeds = [POOL_SEED, pool.collateral_mint.as_ref(), pool.oracle_feed.as_ref()],
        bump = pool.bump,
        has_one = collateral_mint,
        has_one = lp_mint,
        has_one = custody_vault,
        has_one = oracle_feed,
    )]
    pub pool: Box<Account<'info, Pool>>,

    /// CHECK: PDA authority over the vault and liquidity-provider mint.
    #[account(
        seeds = [AUTHORITY_SEED, pool.key().as_ref()],
        bump = pool.authority_bump,
    )]
    pub pool_authority: UncheckedAccount<'info>,

    /// CHECK: validated by the `has_one = oracle_feed` constraint on the pool.
    pub oracle_feed: UncheckedAccount<'info>,

    pub collateral_mint: Box<InterfaceAccount<'info, Mint>>,

    #[account(mut)]
    pub lp_mint: Box<InterfaceAccount<'info, Mint>>,

    #[account(
        mut,
        seeds = [VAULT_SEED, pool.key().as_ref()],
        bump,
    )]
    pub custody_vault: Box<InterfaceAccount<'info, TokenAccount>>,

    #[account(
        mut,
        associated_token::mint = collateral_mint,
        associated_token::authority = provider,
        associated_token::token_program = token_program,
    )]
    pub provider_collateral: Box<InterfaceAccount<'info, TokenAccount>>,

    #[account(
        mut,
        associated_token::mint = lp_mint,
        associated_token::authority = provider,
        associated_token::token_program = token_program,
    )]
    pub provider_lp: Box<InterfaceAccount<'info, TokenAccount>>,

    pub token_program: Interface<'info, TokenInterface>,
    pub associated_token_program: Program<'info, AssociatedToken>,
    pub system_program: Program<'info, System>,
}
