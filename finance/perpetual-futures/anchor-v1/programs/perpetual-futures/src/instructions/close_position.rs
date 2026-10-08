use anchor_lang::prelude::*;
use anchor_spl::{
    associated_token::AssociatedToken,
    token_interface::{transfer_checked, Mint, TokenAccount, TokenInterface, TransferChecked},
};

use crate::constants::{POOL_SEED, POSITION_SEED, VAULT_SEED};
use crate::errors::PerpError;
use crate::instructions::shared::{
    apply_haircut, basis_points_of, credit_fee, haircut_ratio, position_pnl,
    refresh_price_and_funding_within_band, settle_position,
};
use crate::state::{Pool, Position};

pub fn handle_close_position(
    context: Context<ClosePositionAccountConstraints>,
    minimum_payout: u64,
) -> Result<()> {
    let pool = &mut context.accounts.pool;
    let price = refresh_price_and_funding_within_band(pool, &context.accounts.oracle_feed)?;

    // The haircut is computed while this position is still in the per-side
    // accumulators, so its own profit counts toward the liability and it is
    // paid the same fraction as any other winner closing at this price. Its
    // own profit is passed too: if open losers offset it in the aggregate,
    // the haircut is sized against that profit, so the payout is at most the
    // backing and the close is never refused for lack of it.
    let position = &context.accounts.position;
    let closing_profit = position_pnl(position.side, position.size, position.entry_price, price)?;
    let haircut = haircut_ratio(pool, price, closing_profit)?;

    let position_size = position.size;
    let entry_slot = position.entry_slot;
    let settlement = settle_position(pool, position, price)?;
    let close_fee = basis_points_of(position_size, pool.close_fee_bps)?;

    // A profit is paid only once the position has been open for the pool's
    // warm-up, and then only the haircut fraction of it. A loss settles in
    // full, at any time.
    let realized_pnl = if settlement.profit_and_loss > 0 {
        let matured_at = entry_slot
            .checked_add(pool.profit_warmup_slots)
            .ok_or(PerpError::MathOverflow)?;
        require!(
            Clock::get()?.slot >= matured_at,
            PerpError::ProfitNotMatured
        );
        apply_haircut(settlement.profit_and_loss, haircut)?
    } else {
        settlement.profit_and_loss
    };
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

    // Liquidity providers are the counterparty: they pay the trader's
    // haircut profit and receive their loss, and collect the funding the
    // trader owed. The part of a profit the haircut withholds stays in
    // `liquidity`. A payment larger than `liquidity` takes the rest from the
    // insurance fund, which the haircut counted as backing. The haircut keeps
    // the profit within both; `PoolInsolvent` remains as a defensive check.
    let liquidity_delta = settlement
        .funding
        .checked_sub(realized_pnl)
        .ok_or(PerpError::MathOverflow)?;
    let new_liquidity = (pool.liquidity as i128)
        .checked_add(liquidity_delta)
        .ok_or(PerpError::MathOverflow)?;
    if new_liquidity < 0 {
        let shortfall: u64 = new_liquidity
            .unsigned_abs()
            .try_into()
            .map_err(|_| PerpError::MathOverflow)?;
        pool.insurance_fund = pool
            .insurance_fund
            .checked_sub(shortfall)
            .ok_or(PerpError::PoolInsolvent)?;
        pool.liquidity = 0;
    } else {
        pool.liquidity = new_liquidity
            .try_into()
            .map_err(|_| PerpError::MathOverflow)?;
    }
    credit_fee(pool, close_fee)?;

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
                to: context.accounts.trader_collateral.to_account_info(),
                authority: pool.to_account_info(),
            },
            &[pool_seeds],
        ),
        payout,
        context.accounts.collateral_mint.decimals,
    )?;

    Ok(())
}

#[derive(Accounts)]
pub struct ClosePositionAccountConstraints<'info> {
    #[account(mut)]
    pub owner: Signer<'info>,

    #[account(
        mut,
        seeds = [POOL_SEED, pool.collateral_mint.as_ref(), pool.oracle_feed.as_ref()],
        bump = pool.bump,
        has_one = collateral_mint,
        has_one = custody_vault,
        has_one = oracle_feed,
    )]
    pub pool: Box<Account<'info, Pool>>,

    #[account(
        mut,
        close = owner,
        seeds = [POSITION_SEED, pool.key().as_ref(), owner.key().as_ref(), position.side.as_seed()],
        bump = position.bump,
        has_one = owner,
        has_one = pool,
    )]
    pub position: Box<Account<'info, Position>>,

    /// CHECK: validated by the `has_one = oracle_feed` constraint on the pool.
    pub oracle_feed: UncheckedAccount<'info>,

    pub collateral_mint: Box<InterfaceAccount<'info, Mint>>,

    #[account(
        mut,
        seeds = [VAULT_SEED, pool.key().as_ref()],
        bump,
    )]
    pub custody_vault: Box<InterfaceAccount<'info, TokenAccount>>,

    #[account(
        mut,
        associated_token::mint = collateral_mint,
        associated_token::authority = owner,
        associated_token::token_program = token_program,
    )]
    pub trader_collateral: Box<InterfaceAccount<'info, TokenAccount>>,

    pub token_program: Interface<'info, TokenInterface>,
    pub associated_token_program: Program<'info, AssociatedToken>,
    pub system_program: Program<'info, System>,
}
