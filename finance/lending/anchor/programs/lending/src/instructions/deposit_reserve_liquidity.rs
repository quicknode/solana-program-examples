use anchor_lang::prelude::*;
use anchor_spl::token_interface::{
    mint_to, transfer_checked, Mint, MintTo, TokenAccount, TokenInterface, TransferChecked,
};

use crate::errors::LendingError;
use crate::math::{mul_div_floor, reserve_signer_seeds};
use crate::state::Reserve;

/// Supply liquidity to a reserve and receive share tokens. The first deposit
/// mints share tokens 1:1; later deposits mint
/// `liquidity_amount * share_supply / total_liquidity`, floored so the protocol
/// keeps any rounding dust.
pub fn handle_deposit_reserve_liquidity(
    context: Context<DepositReserveLiquidity>,
    liquidity_amount: u64,
) -> Result<()> {
    require!(liquidity_amount > 0, LendingError::ZeroAmount);
    let reserve = &mut context.accounts.reserve;
    reserve.require_refreshed()?;

    let share_supply = reserve.share_mint_supply as u128;
    let share_amount = if share_supply == 0 {
        liquidity_amount as u128
    } else {
        mul_div_floor(liquidity_amount as u128, share_supply, reserve.total_liquidity()?)?
    };
    require!(share_amount > 0, LendingError::DepositTooSmall);
    let share_amount = u64::try_from(share_amount).map_err(|_| LendingError::MathOverflow)?;

    // Effects before interactions.
    reserve.available_liquidity = reserve
        .available_liquidity
        .checked_add(liquidity_amount)
        .ok_or(LendingError::MathOverflow)?;
    reserve.share_mint_supply = reserve
        .share_mint_supply
        .checked_add(share_amount)
        .ok_or(LendingError::MathOverflow)?;

    transfer_checked(
        CpiContext::new(
            context.accounts.token_program.key(),
            TransferChecked {
                from: context.accounts.user_liquidity.to_account_info(),
                mint: context.accounts.liquidity_mint.to_account_info(),
                to: context.accounts.liquidity_vault.to_account_info(),
                authority: context.accounts.owner.to_account_info(),
            },
        ),
        liquidity_amount,
        reserve.liquidity_decimals,
    )?;

    let bump = [reserve.bump];
    let seeds = reserve_signer_seeds(&reserve.lending_market, &reserve.liquidity_mint, &bump);
    mint_to(
        CpiContext::new_with_signer(
            context.accounts.token_program.key(),
            MintTo {
                mint: context.accounts.share_mint.to_account_info(),
                to: context.accounts.user_share.to_account_info(),
                authority: reserve.to_account_info(),
            },
            &[&seeds],
        ),
        share_amount,
    )?;

    Ok(())
}

#[derive(Accounts)]
pub struct DepositReserveLiquidity<'info> {
    #[account(
        mut,
        has_one = liquidity_mint,
        has_one = liquidity_vault,
        has_one = share_mint,
    )]
    pub reserve: Account<'info, Reserve>,

    pub liquidity_mint: InterfaceAccount<'info, Mint>,

    #[account(mut)]
    pub liquidity_vault: InterfaceAccount<'info, TokenAccount>,

    #[account(mut)]
    pub share_mint: InterfaceAccount<'info, Mint>,

    #[account(mut)]
    pub user_liquidity: InterfaceAccount<'info, TokenAccount>,

    #[account(mut)]
    pub user_share: InterfaceAccount<'info, TokenAccount>,

    pub owner: Signer<'info>,

    pub token_program: Interface<'info, TokenInterface>,
}
