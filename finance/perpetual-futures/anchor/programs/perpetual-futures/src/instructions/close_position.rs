use anchor_lang::prelude::*;
use anchor_spl::{
    associated_token::AssociatedToken,
    token_interface::{transfer_checked, Mint, TokenAccount, TokenInterface, TransferChecked},
};

use crate::constants::{AUTHORITY_SEED, POOL_SEED, POSITION_SEED, VAULT_SEED};
use crate::errors::PerpError;
use crate::instructions::shared::{
    apply_haircut, basis_points_of, haircut_ratio, refresh_price_and_funding, settle_position,
    split_fee,
};
use crate::state::{Pool, Position};

pub fn handle_close_position(
    context: Context<ClosePositionAccountConstraints>,
    minimum_payout: u64,
) -> Result<()> {
    let pool = &mut context.accounts.pool;
    let price = refresh_price_and_funding(pool, &context.accounts.oracle_feed)?;

    // Compute the haircut against the whole pool *before* this position leaves
    // the accumulators, so the closer is one of the winners being scaled rather
    // than scaling only those left behind.
    let haircut = haircut_ratio(pool, price)?;

    let position = &context.accounts.position;
    let position_size = position.size;
    let entry_slot = position.entry_slot;
    let settlement = settle_position(pool, position, price)?;
    let close_fee = basis_points_of(position_size, pool.close_fee_bps)?;

    // Profit is a junior claim, gated twice before it is paid; a loss settles in
    // full and skips both gates. First it must have *matured* — the warm-up
    // since open must have elapsed, so a freshly minted oracle gain cannot be
    // cashed out in the block it appears. Then it is *haircut* to the fraction
    // `h` the pool can currently back, the same fraction for every winner.
    let realized_pnl = if settlement.profit_and_loss > 0 {
        let matured =
            Clock::get()?.slot >= entry_slot.saturating_add(pool.profit_warmup_slots);
        require!(matured, PerpError::ProfitNotMatured);
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

    let (insurance_cut, protocol_cut) = split_fee(close_fee, pool.insurance_fee_bps)?;

    // Liquidity providers are the counterparty: they pay the trader's (haircut)
    // profit and receive their loss, and collect the funding the trader owed.
    // The part of a winner's profit withheld by the haircut stays in liquidity —
    // the pool-model way bankruptcy overhang is socialized to providers.
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
        .checked_add(protocol_cut)
        .ok_or(PerpError::MathOverflow)?;
    pool.insurance_fund = pool
        .insurance_fund
        .checked_add(insurance_cut)
        .ok_or(PerpError::MathOverflow)?;

    let pool_key = pool.key();
    let authority_seeds: &[&[u8]] = &[AUTHORITY_SEED, pool_key.as_ref(), &[pool.authority_bump]];
    transfer_checked(
        CpiContext::new_with_signer(
            context.accounts.token_program.key(),
            TransferChecked {
                from: context.accounts.custody_vault.to_account_info(),
                mint: context.accounts.collateral_mint.to_account_info(),
                to: context.accounts.trader_collateral.to_account_info(),
                authority: context.accounts.pool_authority.to_account_info(),
            },
            &[authority_seeds],
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

    /// CHECK: PDA authority over the vault.
    #[account(
        seeds = [AUTHORITY_SEED, pool.key().as_ref()],
        bump = pool.authority_bump,
    )]
    pub pool_authority: UncheckedAccount<'info>,

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
