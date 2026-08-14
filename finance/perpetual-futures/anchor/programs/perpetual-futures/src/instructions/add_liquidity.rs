use anchor_lang::prelude::*;
use anchor_spl::{
    associated_token::AssociatedToken,
    token_interface::{
        mint_to, transfer_checked, Mint, MintTo, TokenAccount, TokenInterface, TransferChecked,
    },
};

use crate::constants::{AUTHORITY_SEED, MINIMUM_LIQUIDITY, POOL_SEED, VAULT_SEED};
use crate::errors::PerpError;
use crate::instructions::shared::{liquidity_provider_aum, refresh_price_and_funding};
use crate::state::Pool;

pub fn handle_add_liquidity(
    context: &mut Context<AddLiquidityAccountConstraints>,
    amount: u64,
    minimum_shares_out: u64,
) -> Result<()> {
    require!(amount > 0, PerpError::ZeroAmount);

    let pool = &mut context.accounts.pool;
    let price = refresh_price_and_funding(pool, &context.accounts.oracle_feed)?;

    let lp_supply = context.accounts.lp_mint.supply();
    let shares: u64 = if lp_supply == 0 {
        // Bootstrap: shares track collateral one-for-one, less the withheld
        // minimum, so the share supply can never start at a dust amount.
        amount
            .checked_sub(MINIMUM_LIQUIDITY)
            .ok_or(PerpError::DepositTooSmall)?
    } else {
        // shares = amount * supply / assets-under-management, floored so the
        // depositor never receives more than their pro-rata claim.
        let aum = liquidity_provider_aum(pool, price)?;
        require!(aum > 0, PerpError::PoolInsolvent);
        (amount as u128)
            .checked_mul(lp_supply as u128)
            .ok_or(PerpError::MathOverflow)?
            .checked_div(aum as u128)
            .ok_or(PerpError::MathOverflow)?
            .try_into()
            .map_err(|_| PerpError::MathOverflow)?
    };

    require!(shares > 0, PerpError::AmountRoundsToZero);
    require!(shares >= minimum_shares_out, PerpError::SlippageExceeded);

    // Effects before interactions: record the new liquidity, then move tokens.
    pool.liquidity = pool
        .liquidity
        .checked_add(amount)
        .ok_or(PerpError::MathOverflow)?;

    transfer_checked(
        CpiContext::new(
            context.accounts.token_program.address(),
            TransferChecked {
                from: context.accounts.provider_collateral.cpi_handle_mut(),
                mint: context.accounts.collateral_mint.cpi_handle(),
                to: context.accounts.custody_vault.cpi_handle_mut(),
                authority: context.accounts.provider.cpi_handle(),
            },
        ),
        amount,
        context.accounts.collateral_mint.decimals(),
    )?;

    let pool_key = pool.address();
    let authority_seeds: &[&[u8]] = &[AUTHORITY_SEED, pool_key.as_ref(), &[pool.authority_bump]];
    mint_to(
        CpiContext::new_with_signer(
            context.accounts.token_program.address(),
            MintTo {
                mint: context.accounts.lp_mint.cpi_handle_mut(),
                to: context.accounts.provider_lp.cpi_handle_mut(),
                authority: context.accounts.pool_authority.cpi_handle(),
            },
            &[authority_seeds],
        ),
        shares,
    )?;

    Ok(())
}

#[derive(Accounts)]
pub struct AddLiquidityAccountConstraints {
    #[account(mut)]
    pub provider: Signer,

    #[account(
        mut,
        seeds = [POOL_SEED, pool.collateral_mint.as_ref(), pool.oracle_feed.as_ref()],
        bump = pool.bump,
        has_one = collateral_mint,
        has_one = lp_mint,
        has_one = custody_vault,
        has_one = oracle_feed,
    )]
    pub pool: Box<BorshAccount<Pool>>,

    /// CHECK: PDA authority over the vault and liquidity-provider mint.
    #[account(
        seeds = [AUTHORITY_SEED, pool.address().as_ref()],
        bump = pool.authority_bump,
    )]
    pub pool_authority: UncheckedAccount,

    /// CHECK: validated by the `has_one = oracle_feed` constraint on the pool.
    pub oracle_feed: UncheckedAccount,

    pub collateral_mint: Box<InterfaceAccount<Mint>>,

    #[account(mut)]
    pub lp_mint: Box<InterfaceAccount<Mint>>,

    #[account(
        mut,
        seeds = [VAULT_SEED, pool.address().as_ref()],
        bump,
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
        init_if_needed,
        payer = provider,
        associated_token::mint = lp_mint,
        associated_token::authority = provider,
        associated_token::token_program = token_program,
    )]
    pub provider_lp: Box<InterfaceAccount<TokenAccount>>,

    pub token_program: Interface<'static, TokenInterface>,
    pub associated_token_program: Program<AssociatedToken>,
    pub system_program: Program<System>,
}
