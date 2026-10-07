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
    context: Context<RemoveLiquidityAccountConstraints>,
    shares: u64,
    minimum_amount_out: u64,
) -> Result<()> {
    require!(shares > 0, PerpError::ZeroAmount);

    let pool = &mut context.accounts.pool;
    let price = refresh_price_and_funding_within_band(pool, &context.accounts.oracle_feed)?;

    let lp_supply = context.accounts.lp_mint.supply;
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
            context.accounts.token_program.key(),
            Burn {
                mint: context.accounts.lp_mint.to_account_info(),
                from: context.accounts.provider_lp.to_account_info(),
                authority: context.accounts.provider.to_account_info(),
            },
        ),
        shares,
    )?;

    // The pool signs the CPI below with its own seeds.
    let pool_seeds: &[&[u8]] = &[
        POOL_SEED,
        pool.collateral_mint.as_ref(),
        pool.oracle_feed.as_ref(),
        &[pool.bump],
    ];
    transfer_checked(
        CpiContext::new_with_signer(
            context.accounts.token_program.key(),
            TransferChecked {
                from: context.accounts.custody_vault.to_account_info(),
                mint: context.accounts.collateral_mint.to_account_info(),
                to: context.accounts.provider_collateral.to_account_info(),
                authority: pool.to_account_info(),
            },
            &[pool_seeds],
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
