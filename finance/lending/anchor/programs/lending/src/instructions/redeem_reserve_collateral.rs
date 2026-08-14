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
    context: &mut Context<RedeemReserveCollateral>,
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
    let liquidity_amount =
        u64::try_from(liquidity_amount).map_err(|_| LendingError::MathOverflow)?;
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
            context.accounts.token_program.address(),
            Burn {
                mint: context.accounts.share_mint.cpi_handle_mut(),
                from: context.accounts.user_share.cpi_handle_mut(),
                authority: context.accounts.owner.cpi_handle(),
            },
        ),
        share_amount,
    )?;

    let bump = [reserve.bump];
    let seeds = reserve_signer_seeds(&reserve.lending_market, &reserve.liquidity_mint, &bump);
    transfer_checked(
        CpiContext::new_with_signer(
            context.accounts.token_program.address(),
            TransferChecked {
                from: context.accounts.liquidity_vault.cpi_handle_mut(),
                mint: context.accounts.liquidity_mint.cpi_handle(),
                to: context.accounts.user_liquidity.cpi_handle_mut(),
                authority: reserve.cpi_handle(),
            },
            &[&seeds],
        ),
        liquidity_amount,
        reserve.liquidity_decimals,
    )?;

    Ok(())
}

#[derive(Accounts)]
pub struct RedeemReserveCollateral {
    #[account(mut)]
    pub reserve: BorshAccount<Reserve>,

    #[account(address = reserve.liquidity_mint)]
    pub liquidity_mint: InterfaceAccount<Mint>,

    #[account(mut, address = reserve.liquidity_vault)]
    pub liquidity_vault: InterfaceAccount<TokenAccount>,

    #[account(mut, address = reserve.share_mint)]
    pub share_mint: InterfaceAccount<Mint>,

    #[account(mut)]
    pub user_liquidity: InterfaceAccount<TokenAccount>,

    #[account(mut)]
    pub user_share: InterfaceAccount<TokenAccount>,

    pub owner: Signer,

    pub token_program: Interface<'static, TokenInterface>,
}
