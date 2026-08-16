use anchor_lang::prelude::*;
use anchor_spl::{
    associated_token::AssociatedToken,
    token_interface::{transfer_checked, Mint, TokenAccount, TokenInterface, TransferChecked},
};

use crate::constants::{AUTHORITY_SEED, POOL_SEED, POSITION_SEED, VAULT_SEED};
use crate::errors::PerpError;
use crate::instructions::shared::{basis_points_of, refresh_price_and_funding, settle_position};
use crate::state::{Pool, Position};

pub fn handle_close_position(
    context: &mut Context<ClosePositionAccountConstraints>,
    minimum_payout: u64,
) -> Result<()> {
    let pool = &mut context.accounts.pool;
    let price = refresh_price_and_funding(pool, &context.accounts.oracle_feed)?;

    let position = &context.accounts.position;
    let position_size = position.size;
    let settlement = settle_position(pool, position, price)?;
    let close_fee = basis_points_of(position_size, pool.close_fee_bps)?;

    // Recoverable profit is capped at the reserved amount (the position's
    // notional `size`), so the pool can always cover a winner. Losses are not
    // capped.
    let realized_pnl = settlement.profit_and_loss.min(position_size as i128);
    let equity = settlement
        .equity
        .checked_sub(settlement.profit_and_loss)
        .ok_or(PerpError::MathOverflow)?
        .checked_add(realized_pnl)
        .ok_or(PerpError::MathOverflow)?;

    // The trader receives their equity minus the close fee. A non-positive
    // payout means the position is underwater and must go through liquidation,
    // not a voluntary close.
    let payout = equity
        .checked_sub(close_fee as i128)
        .ok_or(PerpError::MathOverflow)?;
    require!(payout > 0, PerpError::PositionNotHealthy);
    let payout: u64 = payout.try_into().map_err(|_| PerpError::MathOverflow)?;
    require!(payout >= minimum_payout, PerpError::SlippageExceeded);

    // Release the position's reserved liquidity now that it is closing.
    pool.reserved_liquidity = pool
        .reserved_liquidity
        .checked_sub(position_size)
        .ok_or(PerpError::MathOverflow)?;

    // Liquidity providers are the counterparty: they pay the trader's (capped)
    // profit and receive their loss, and collect the funding the trader owed.
    let liquidity_delta = settlement
        .funding
        .checked_sub(realized_pnl)
        .ok_or(PerpError::MathOverflow)?;
    let new_liquidity = (pool.liquidity as i128)
        .checked_add(liquidity_delta)
        .ok_or(PerpError::MathOverflow)?;
    require!(new_liquidity >= 0, PerpError::PoolInsolvent);
    pool.liquidity = new_liquidity
        .try_into()
        .map_err(|_| PerpError::MathOverflow)?;
    pool.protocol_fees = pool
        .protocol_fees
        .checked_add(close_fee)
        .ok_or(PerpError::MathOverflow)?;

    let pool_key = pool.address();
    let authority_seeds: &[&[u8]] = &[AUTHORITY_SEED, pool_key.as_ref(), &[pool.authority_bump]];
    transfer_checked(
        CpiContext::new_with_signer(
            context.accounts.token_program.address(),
            TransferChecked {
                from: context.accounts.custody_vault.to_cpi_handle_mut(),
                mint: context.accounts.collateral_mint.to_cpi_handle(),
                to: context.accounts.trader_collateral.to_cpi_handle_mut(),
                authority: context.accounts.pool_authority.cpi_handle(),
            },
            &[authority_seeds],
        ),
        payout,
        context.accounts.collateral_mint.decimals(),
    )?;

    Ok(())
}

#[derive(Accounts)]
pub struct ClosePositionAccountConstraints {
    #[account(mut, address = position.owner)]
    pub owner: Signer,

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

    pub token_program: Interface<'static, TokenInterface>,
    pub associated_token_program: Program<AssociatedToken>,
    pub system_program: Program<System>,
}
