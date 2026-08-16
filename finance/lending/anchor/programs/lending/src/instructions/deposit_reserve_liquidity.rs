use anchor_lang::prelude::*;
use anchor_spl::token_interface::{
    mint_to, transfer_checked, Mint, MintTo, TokenAccount, TokenInterface, TransferChecked,
};

use crate::errors::LendingError;
use crate::math::mul_div_floor;
use crate::state::{reserve_signer_seeds, Reserve};

/// Supply liquidity to a reserve and receive share tokens. The first deposit
/// mints share tokens 1:1; later deposits mint
/// `liquidity_amount * share_supply / total_liquidity`, floored so the protocol
/// keeps any rounding dust.
pub fn handle_deposit_reserve_liquidity(
    context: &mut Context<DepositReserveLiquidity>,
    liquidity_amount: u64,
) -> Result<()> {
    require!(liquidity_amount > 0, LendingError::ZeroAmount);
    let reserve = &mut context.accounts.reserve;
    reserve.require_refreshed()?;

    let share_supply = reserve.share_mint_supply as u128;
    let share_amount = if share_supply == 0 {
        liquidity_amount as u128
    } else {
        mul_div_floor(
            liquidity_amount as u128,
            share_supply,
            reserve.total_liquidity()?,
        )?
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
            context.accounts.token_program.address(),
            TransferChecked {
                from: context.accounts.user_liquidity.cpi_handle_mut(),
                mint: context.accounts.liquidity_mint.cpi_handle(),
                to: context.accounts.liquidity_vault.cpi_handle_mut(),
                authority: context.accounts.owner.cpi_handle(),
            },
        ),
        liquidity_amount,
        reserve.liquidity_decimals,
    )?;

    // Copy the seed inputs out: `release_borrow` below needs `&mut reserve`.
    let bump = [reserve.bump];
    let lending_market = reserve.lending_market;
    let liquidity_mint = reserve.liquidity_mint;
    let seeds = reserve_signer_seeds(&lending_market, &liquidity_mint, &bump);
    // `reserve` signs this CPI. It is a data account holding a live borrow on
    // its buffer, which the runtime would reject when the CPI borrows the same
    // account — so hand the borrow back across the call. `release_borrow`
    // flushes the pending writes, and `reacquire_borrow_mut` re-reads them.
    context.accounts.reserve.release_borrow()?;
    mint_to(
        CpiContext::new_with_signer(
            context.accounts.token_program.address(),
            MintTo {
                mint: context.accounts.share_mint.cpi_handle_mut(),
                to: context.accounts.user_share.cpi_handle_mut(),
                authority: context.accounts.reserve.cpi_handle(),
            },
            &[&seeds],
        ),
        share_amount,
    )?;
    context.accounts.reserve.reacquire_borrow_mut()?;

    Ok(())
}

#[derive(Accounts)]
pub struct DepositReserveLiquidity {
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
