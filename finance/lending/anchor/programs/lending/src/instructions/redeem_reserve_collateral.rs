use anchor_lang::prelude::*;
use anchor_spl::token_interface::{
    burn, transfer_checked, Burn, Mint, TokenAccount, TokenInterface, TransferChecked,
};

use crate::errors::LendingError;
use crate::math::mul_div_floor;
use crate::state::{reserve_signer_seeds, Reserve};

/// Burn share tokens and withdraw the underlying liquidity they represent:
/// `share_amount * total_liquidity / share_supply`, floored so the protocol
/// keeps any rounding dust. Capped by the reserve's available (un-borrowed)
/// liquidity.
pub fn handle_redeem_reserve_collateral(
    context: Context<RedeemReserveCollateral>,
    share_amount: u64,
) -> Result<()> {
    require!(share_amount > 0, LendingError::ZeroAmount);
    let reserve = &mut context.accounts.reserve;
    reserve.require_refreshed()?;

    let share_supply = reserve.share_mint_supply as u128;
    require!(share_supply > 0, LendingError::InsufficientReserveLiquidity);
    let liquidity_amount = mul_div_floor(
        share_amount as u128,
        reserve.total_liquidity()?,
        share_supply,
    )?;
    let liquidity_amount = u64::try_from(liquidity_amount).map_err(|_| LendingError::MathOverflow)?;
    require!(
        liquidity_amount <= reserve.available_liquidity,
        LendingError::InsufficientReserveLiquidity
    );

    reserve.available_liquidity = reserve
        .available_liquidity
        .checked_sub(liquidity_amount)
        .ok_or(LendingError::MathOverflow)?;
    reserve.share_mint_supply = reserve
        .share_mint_supply
        .checked_sub(share_amount)
        .ok_or(LendingError::MathOverflow)?;

    burn(
        CpiContext::new(
            context.accounts.token_program.key(),
            Burn {
                mint: context.accounts.share_mint.to_account_info(),
                from: context.accounts.user_share.to_account_info(),
                authority: context.accounts.owner.to_account_info(),
            },
        ),
        share_amount,
    )?;

    let bump = [reserve.bump];
    let seeds = reserve_signer_seeds(&reserve.lending_market, &reserve.liquidity_mint, &bump);
    transfer_checked(
        CpiContext::new_with_signer(
            context.accounts.token_program.key(),
            TransferChecked {
                from: context.accounts.liquidity_vault.to_account_info(),
                mint: context.accounts.liquidity_mint.to_account_info(),
                to: context.accounts.user_liquidity.to_account_info(),
                authority: reserve.to_account_info(),
            },
            &[&seeds],
        ),
        liquidity_amount,
        reserve.liquidity_decimals,
    )?;

    Ok(())
}

#[derive(Accounts)]
pub struct RedeemReserveCollateral<'info> {
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
