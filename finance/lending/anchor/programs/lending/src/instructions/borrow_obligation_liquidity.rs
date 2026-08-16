use anchor_lang::prelude::*;
use anchor_spl::token_interface::{
    transfer_checked, Mint, TokenAccount, TokenInterface, TransferChecked,
};

use crate::constants::FIXED_POINT_SCALE;
use crate::errors::LendingError;
use crate::math::{market_value, mul_div_ceil, Rounding};
use crate::state::{reserve_signer_seeds, Obligation, PriceFeed, Reserve};

/// Borrow liquidity against the obligation's collateral. The new debt's value
/// (rounded up) plus the existing debt must stay within the obligation's
/// allowed-borrow value. The borrowed amount is recorded as scaled principal at
/// the reserve's current index (rounded up) so it accrues interest going forward.
pub fn handle_borrow_obligation_liquidity(
    context: &mut Context<BorrowObligationLiquidity>,
    liquidity_amount: u64,
) -> Result<()> {
    require!(liquidity_amount > 0, LendingError::ZeroAmount);
    let slot = Clock::get()?.slot;
    let reserve_key = *context.accounts.reserve.address();

    context.accounts.obligation.require_refreshed()?;
    context.accounts.reserve.require_refreshed()?;

    let price_scaled = context.accounts.price_feed.price_scaled(slot)?;
    let decimals = context.accounts.reserve.liquidity_decimals;
    let borrow_value = market_value(liquidity_amount, decimals, price_scaled, Rounding::Up)?;

    let projected_borrowed_value = context
        .accounts
        .obligation
        .borrowed_value
        .checked_add(borrow_value)
        .ok_or(LendingError::MathOverflow)?;
    require!(
        projected_borrowed_value <= context.accounts.obligation.allowed_borrow_value,
        LendingError::BorrowTooLarge
    );
    require!(
        liquidity_amount <= context.accounts.reserve.available_liquidity,
        LendingError::InsufficientReserveLiquidity
    );

    let scaled_added = mul_div_ceil(
        liquidity_amount as u128,
        FIXED_POINT_SCALE,
        context.accounts.reserve.borrow_accumulation_factor,
    )?;

    {
        let reserve = &mut context.accounts.reserve;
        reserve.borrowed_principal = reserve
            .borrowed_principal
            .checked_add(scaled_added)
            .ok_or(LendingError::MathOverflow)?;
        reserve.available_liquidity = reserve
            .available_liquidity
            .checked_sub(liquidity_amount)
            .ok_or(LendingError::MathOverflow)?;
    }

    {
        let obligation = &mut context.accounts.obligation;
        let index = obligation.upsert_borrow(reserve_key)?;
        obligation.borrows[index].borrowed_principal = obligation.borrows[index]
            .borrowed_principal
            .checked_add(scaled_added)
            .ok_or(LendingError::MathOverflow)?;
        obligation.stale = true;
    }

    let reserve = &context.accounts.reserve;
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
        decimals,
    )?;

    Ok(())
}

#[derive(Accounts)]
pub struct BorrowObligationLiquidity {
    #[account(mut)]
    pub obligation: BorshAccount<Obligation>,

    #[account(address = obligation.owner)]
    pub owner: Signer,

    #[account(
        mut,
        constraint = reserve.lending_market == obligation.lending_market @ LendingError::MarketMismatch,
    )]
    pub reserve: BorshAccount<Reserve>,

    #[account(address = reserve.price_feed)]
    pub price_feed: BorshAccount<PriceFeed>,

    #[account(address = reserve.liquidity_mint)]
    pub liquidity_mint: InterfaceAccount<Mint>,

    #[account(mut, address = reserve.liquidity_vault)]
    pub liquidity_vault: InterfaceAccount<TokenAccount>,

    #[account(mut)]
    pub user_liquidity: InterfaceAccount<TokenAccount>,

    pub token_program: Interface<'static, TokenInterface>,
}
