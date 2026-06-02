use anchor_lang::prelude::*;
use anchor_spl::token_interface::{
    transfer_checked, Mint, TokenAccount, TokenInterface, TransferChecked,
};

use crate::constants::FIXED_POINT_SCALE;
use crate::errors::LendingError;
use crate::math::{mul_div_ceil, mul_div_floor};
use crate::state::{Obligation, Reserve};

/// Repay borrowed liquidity, clamped to the live debt. The repaid amount removes
/// scaled principal rounded down, so any sub-unit of principal lingers with the
/// borrower rather than being forgiven by rounding. Anyone may repay on behalf
/// of an obligation, so there is no owner check.
pub fn handle_repay_obligation_liquidity(
    context: Context<RepayObligationLiquidity>,
    liquidity_amount: u64,
) -> Result<()> {
    require!(liquidity_amount > 0, LendingError::ZeroAmount);
    let reserve_key = context.accounts.reserve.key();
    context.accounts.reserve.require_refreshed()?;

    let index = context.accounts.reserve.cumulative_borrow_rate_index;
    let decimals = context.accounts.reserve.liquidity_decimals;

    let borrow_index = context.accounts.obligation.find_borrow(reserve_key)?;
    let borrowed_scaled = context.accounts.obligation.borrows[borrow_index].borrowed_scaled;

    let debt_now = mul_div_ceil(borrowed_scaled, index, FIXED_POINT_SCALE)?;
    let debt_now = u64::try_from(debt_now).map_err(|_| LendingError::MathOverflow)?;
    let repay = liquidity_amount.min(debt_now);
    require!(repay > 0, LendingError::ZeroAmount);

    let scaled_removed = mul_div_floor(repay as u128, FIXED_POINT_SCALE, index)?.min(borrowed_scaled);

    {
        let reserve = &mut context.accounts.reserve;
        reserve.borrowed_amount_scaled = reserve
            .borrowed_amount_scaled
            .checked_sub(scaled_removed)
            .ok_or(LendingError::MathOverflow)?;
        reserve.available_liquidity = reserve
            .available_liquidity
            .checked_add(repay)
            .ok_or(LendingError::MathOverflow)?;
    }

    {
        let obligation = &mut context.accounts.obligation;
        obligation.borrows[borrow_index].borrowed_scaled = borrowed_scaled
            .checked_sub(scaled_removed)
            .ok_or(LendingError::MathOverflow)?;
        if obligation.borrows[borrow_index].borrowed_scaled == 0 {
            obligation.borrows.remove(borrow_index);
        }
        obligation.stale = true;
    }

    transfer_checked(
        CpiContext::new(
            context.accounts.token_program.key(),
            TransferChecked {
                from: context.accounts.user_liquidity.to_account_info(),
                mint: context.accounts.liquidity_mint.to_account_info(),
                to: context.accounts.liquidity_vault.to_account_info(),
                authority: context.accounts.repayer.to_account_info(),
            },
        ),
        repay,
        decimals,
    )?;

    Ok(())
}

#[derive(Accounts)]
pub struct RepayObligationLiquidity<'info> {
    #[account(mut)]
    pub obligation: Account<'info, Obligation>,

    #[account(
        mut,
        has_one = liquidity_mint,
        has_one = liquidity_vault,
    )]
    pub reserve: Account<'info, Reserve>,

    pub liquidity_mint: InterfaceAccount<'info, Mint>,

    #[account(mut)]
    pub liquidity_vault: InterfaceAccount<'info, TokenAccount>,

    #[account(mut)]
    pub user_liquidity: InterfaceAccount<'info, TokenAccount>,

    pub repayer: Signer<'info>,

    pub token_program: Interface<'info, TokenInterface>,
}
