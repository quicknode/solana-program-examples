use anchor_lang::prelude::*;
use anchor_spl::{
    associated_token::AssociatedToken,
    token_interface::{transfer_checked, Mint, TokenAccount, TokenInterface, TransferChecked},
};

use crate::constants::{AUTHORITY_SEED, POOL_SEED, POSITION_SEED, VAULT_SEED};
use crate::errors::PerpError;
use crate::instructions::shared::{basis_points_of, refresh_price_and_funding, settle_position};
use crate::state::{Pool, Position};

pub fn handle_liquidate_position(
    context: &mut Context<LiquidatePositionAccountConstraints>,
) -> Result<()> {
    let pool = &mut context.accounts.pool;
    let price = refresh_price_and_funding(pool, &context.accounts.oracle_feed)?;

    let position = &context.accounts.position;
    let position_size = position.size;
    let settlement = settle_position(pool, position, price)?;

    // Release the position's reserved liquidity now that it is closing.
    pool.reserved_liquidity = pool
        .reserved_liquidity
        .checked_sub(position_size)
        .ok_or(PerpError::MathOverflow)?;

    // Liquidatable only once equity has fallen to or below the maintenance
    // margin. A healthy position can only be closed by its owner.
    let maintenance = basis_points_of(position_size, pool.maintenance_margin_bps)?;
    require!(
        settlement.equity <= maintenance as i128,
        PerpError::PositionHealthy
    );

    // The liquidator's reward comes out of whatever equity remains, capped so a
    // position already past zero equity cannot pay out more than it has.
    let remaining_equity: u64 = settlement
        .equity
        .max(0)
        .try_into()
        .map_err(|_| PerpError::MathOverflow)?;
    let liquidation_fee = basis_points_of(position.size, pool.liquidation_fee_bps)?;
    let liquidator_payout = liquidation_fee.min(remaining_equity);
    let trader_refund = remaining_equity
        .checked_sub(liquidator_payout)
        .ok_or(PerpError::MathOverflow)?;

    // Everything the trader does not get back stays with the liquidity
    // providers. Derived from vault conservation: the pool keeps the position's
    // collateral minus whatever is paid out as equity.
    let liquidity_delta = (position.collateral as i128)
        .checked_sub(remaining_equity as i128)
        .ok_or(PerpError::MathOverflow)?;
    let new_liquidity = (pool.liquidity as i128)
        .checked_add(liquidity_delta)
        .ok_or(PerpError::MathOverflow)?;
    require!(new_liquidity >= 0, PerpError::PoolInsolvent);
    pool.liquidity = new_liquidity
        .try_into()
        .map_err(|_| PerpError::MathOverflow)?;

    let pool_key = pool.address();
    let authority_seeds: &[&[u8]] = &[AUTHORITY_SEED, pool_key.as_ref(), &[pool.authority_bump]];

    if liquidator_payout > 0 {
        transfer_checked(
            CpiContext::new_with_signer(
                context.accounts.token_program.address(),
                TransferChecked {
                    from: context.accounts.custody_vault.cpi_handle_mut(),
                    mint: context.accounts.collateral_mint.cpi_handle(),
                    to: context.accounts.liquidator_collateral.cpi_handle_mut(),
                    authority: context.accounts.pool_authority.cpi_handle(),
                },
                &[authority_seeds],
            ),
            liquidator_payout,
            context.accounts.collateral_mint.decimals(),
        )?;
    }

    if trader_refund > 0 {
        transfer_checked(
            CpiContext::new_with_signer(
                context.accounts.token_program.address(),
                TransferChecked {
                    from: context.accounts.custody_vault.cpi_handle_mut(),
                    mint: context.accounts.collateral_mint.cpi_handle(),
                    to: context.accounts.trader_collateral.cpi_handle_mut(),
                    authority: context.accounts.pool_authority.cpi_handle(),
                },
                &[authority_seeds],
            ),
            trader_refund,
            context.accounts.collateral_mint.decimals(),
        )?;
    }

    Ok(())
}

#[derive(Accounts)]
pub struct LiquidatePositionAccountConstraints {
    #[account(mut)]
    pub liquidator: Signer,

    /// CHECK: the position owner, validated by `address = position.owner` below.
    /// Receives the position account's rent and any equity refund.
    #[account(mut, address = position.owner)]
    pub owner: UncheckedAccount,

    #[account(
        mut,
        seeds = [POOL_SEED, pool.collateral_mint.as_ref(), pool.oracle_feed.as_ref()],
        bump = pool.bump,
        address = position.pool,
    )]
    pub pool: Box<BorshAccount<Pool>>,

    #[account(
        mut,
        close = owner,
        seeds = [POSITION_SEED, pool.address().as_ref(), owner.address().as_ref(), position.side.as_seed()],
        bump = position.bump,
    )]
    pub position: Box<BorshAccount<Position>>,

    /// CHECK: PDA authority over the vault.
    #[account(
        seeds = [AUTHORITY_SEED, pool.address().as_ref()],
        bump = pool.authority_bump,
    )]
    pub pool_authority: UncheckedAccount,

    /// CHECK: validated by the `address = pool.oracle_feed` constraint below.
    #[account(address = pool.oracle_feed)]
    pub oracle_feed: UncheckedAccount,

    #[account(address = pool.collateral_mint)]
    pub collateral_mint: Box<InterfaceAccount<Mint>>,

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
        associated_token::authority = owner,
        associated_token::token_program = token_program,
    )]
    pub trader_collateral: Box<InterfaceAccount<TokenAccount>>,

    #[account(
        init_if_needed,
        payer = liquidator,
        associated_token::mint = collateral_mint,
        associated_token::authority = liquidator,
        associated_token::token_program = token_program,
    )]
    pub liquidator_collateral: Box<InterfaceAccount<TokenAccount>>,

    pub token_program: Interface<'static, TokenInterface>,
    pub associated_token_program: Program<AssociatedToken>,
    pub system_program: Program<System>,
}
