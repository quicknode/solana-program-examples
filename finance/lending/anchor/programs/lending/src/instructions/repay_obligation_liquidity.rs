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
    context: &mut Context<RepayObligationLiquidity>,
    liquidity_amount: u64,
) -> Result<()> {
    require!(liquidity_amount > 0, LendingError::ZeroAmount);
    let reserve_key = context.accounts.reserve.address();
    context.accounts.reserve.require_refreshed()?;

    let index = context.accounts.reserve.borrow_accumulation_factor;
    let decimals = context.accounts.reserve.liquidity_decimals;

    let borrow_index = context.accounts.obligation.find_borrow(*reserve_key)?;
    let borrowed_principal = context.accounts.obligation.borrows[borrow_index].borrowed_principal;

    let debt_now = mul_div_ceil(borrowed_principal, index, FIXED_POINT_SCALE)?;
    let debt_now = u64::try_from(debt_now).map_err(|_| LendingError::MathOverflow)?;
    let repay = liquidity_amount.min(debt_now);
    require!(repay > 0, LendingError::ZeroAmount);

    let scaled_removed =
        mul_div_floor(repay as u128, FIXED_POINT_SCALE, index)?.min(borrowed_principal);

    {
        let reserve = &mut context.accounts.reserve;
        reserve.borrowed_principal = reserve
            .borrowed_principal
            .checked_sub(scaled_removed)
            .ok_or(LendingError::MathOverflow)?;
        reserve.available_liquidity = reserve
            .available_liquidity
            .checked_add(repay)
            .ok_or(LendingError::MathOverflow)?;
    }

    {
        let obligation = &mut context.accounts.obligation;
        obligation.borrows[borrow_index].borrowed_principal = borrowed_principal
            .checked_sub(scaled_removed)
            .ok_or(LendingError::MathOverflow)?;
        if obligation.borrows[borrow_index].borrowed_principal == 0 {
            obligation.borrows.remove(borrow_index);
        }
        obligation.stale = true;
    }

    transfer_checked(
        CpiContext::new(
            context.accounts.token_program.address(),
            TransferChecked {
                from: context.accounts.user_liquidity.cpi_handle_mut(),
                mint: context.accounts.liquidity_mint.cpi_handle(),
                to: context.accounts.liquidity_vault.cpi_handle_mut(),
                authority: context.accounts.repayer.cpi_handle(),
            },
        ),
        repay,
        decimals,
    )?;

    Ok(())
}

#[derive(Accounts)]
pub struct RepayObligationLiquidity {
    #[account(mut)]
    pub obligation: BorshAccount<Obligation>,

    #[account(
        mut,
        has_one = liquidity_mint,
        has_one = liquidity_vault,
        constraint = reserve.lending_market == obligation.lending_market @ LendingError::MarketMismatch,
    )]
    pub reserve: BorshAccount<Reserve>,

    pub liquidity_mint: InterfaceAccount<Mint>,

    #[account(mut)]
    pub liquidity_vault: InterfaceAccount<TokenAccount>,

    #[account(mut)]
    pub user_liquidity: InterfaceAccount<TokenAccount>,

    pub repayer: Signer,

    pub token_program: Interface<'static, TokenInterface>,
}
