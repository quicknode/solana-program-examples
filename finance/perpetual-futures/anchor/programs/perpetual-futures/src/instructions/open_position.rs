use anchor_lang::prelude::*;
use anchor_spl::{
    associated_token::AssociatedToken,
    token_interface::{transfer_checked, Mint, TokenAccount, TokenInterface, TransferChecked},
};

use crate::constants::{POOL_SEED, POSITION_SEED, VAULT_SEED};
use crate::errors::PerpError;
use crate::instructions::shared::{basis_points_of, refresh_price_and_funding, scale_size, split_fee};
use crate::state::{Pool, Position, Side};

pub fn handle_open_position(
    context: Context<OpenPositionAccountConstraints>,
    side: Side,
    collateral_amount: u64,
    size: u64,
    acceptable_price: u64,
) -> Result<()> {
    require!(collateral_amount > 0 && size > 0, PerpError::ZeroAmount);

    let pool = &mut context.accounts.pool;
    let price = refresh_price_and_funding(pool, &context.accounts.oracle_feed)?;

    // Slippage: a long must not fill above the caller's limit, a short not
    // below it. `0` opts out.
    if acceptable_price != 0 {
        let acceptable = match side {
            Side::Long => price <= acceptable_price,
            Side::Short => price >= acceptable_price,
        };
        require!(acceptable, PerpError::SlippageExceeded);
    }

    // The open fee is taken out of the posted collateral; the rest backs the
    // position. Leverage and margin are measured against this net collateral.
    let open_fee = basis_points_of(size, pool.open_fee_bps)?;
    let net_collateral = collateral_amount
        .checked_sub(open_fee)
        .ok_or(PerpError::InsufficientCollateral)?;
    require!(net_collateral > 0, PerpError::ZeroAmount);

    let max_notional = (net_collateral as u128)
        .checked_mul(pool.max_leverage as u128)
        .ok_or(PerpError::MathOverflow)?;
    require!(size as u128 <= max_notional, PerpError::LeverageTooHigh);

    // Refuse a position that would open already inside the liquidation band.
    let maintenance = basis_points_of(size, pool.maintenance_margin_bps)?;
    require!(net_collateral > maintenance, PerpError::PositionNotHealthy);

    // No open-interest cap: a position can open even when the pool could not
    // cover its full winnings. Profit is a junior claim — if traders end up
    // collectively owed more than the pool holds, the haircut `h` scales every
    // winner's profit to the available backing rather than reserving capital up
    // front. Solvency is preserved at exit, not gated at entry.
    let size_scaled = scale_size(size, price)?;
    let (insurance_cut, protocol_cut) = split_fee(open_fee, pool.insurance_fee_bps)?;

    // Effects: record the position and the pool's new aggregates before moving
    // any tokens.
    let position = &mut context.accounts.position;
    position.owner = context.accounts.owner.key();
    position.pool = pool.key();
    position.side = side;
    position.collateral = net_collateral;
    position.size = size;
    position.entry_price = price;
    position.size_scaled = size_scaled;
    position.entry_funding = pool.cumulative_funding;
    position.entry_slot = Clock::get()?.slot;
    position.bump = context.bumps.position;

    pool.total_collateral = pool
        .total_collateral
        .checked_add(net_collateral)
        .ok_or(PerpError::MathOverflow)?;
    pool.protocol_fees = pool
        .protocol_fees
        .checked_add(protocol_cut)
        .ok_or(PerpError::MathOverflow)?;
    pool.insurance_fund = pool
        .insurance_fund
        .checked_add(insurance_cut)
        .ok_or(PerpError::MathOverflow)?;

    match side {
        Side::Long => {
            pool.long_size = pool
                .long_size
                .checked_add(size as u128)
                .ok_or(PerpError::MathOverflow)?;
            pool.long_size_scaled = pool
                .long_size_scaled
                .checked_add(size_scaled)
                .ok_or(PerpError::MathOverflow)?;
        }
        Side::Short => {
            pool.short_size = pool
                .short_size
                .checked_add(size as u128)
                .ok_or(PerpError::MathOverflow)?;
            pool.short_size_scaled = pool
                .short_size_scaled
                .checked_add(size_scaled)
                .ok_or(PerpError::MathOverflow)?;
        }
    }

    transfer_checked(
        CpiContext::new(
            context.accounts.token_program.key(),
            TransferChecked {
                from: context.accounts.trader_collateral.to_account_info(),
                mint: context.accounts.collateral_mint.to_account_info(),
                to: context.accounts.custody_vault.to_account_info(),
                authority: context.accounts.owner.to_account_info(),
            },
        ),
        collateral_amount,
        context.accounts.collateral_mint.decimals,
    )?;

    Ok(())
}

#[derive(Accounts)]
#[instruction(side: Side)]
pub struct OpenPositionAccountConstraints<'info> {
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
        init,
        payer = owner,
        space = Position::DISCRIMINATOR.len() + Position::INIT_SPACE,
        seeds = [POSITION_SEED, pool.key().as_ref(), owner.key().as_ref(), side.as_seed()],
        bump,
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
