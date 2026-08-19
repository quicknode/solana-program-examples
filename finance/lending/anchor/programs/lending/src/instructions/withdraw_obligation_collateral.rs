use anchor_lang::prelude::*;
use anchor_spl::token;
use anchor_spl::token_interface::{
    transfer_checked, Mint, TokenAccount, TokenInterface, TransferChecked,
};

use crate::constants::{BPS_DENOMINATOR, OBLIGATION_SEED, OBLIGATION_SHARE_VAULT_SEED};
use crate::errors::LendingError;
use crate::math::{market_value, mul_div_ceil, Rounding};
use crate::state::{Obligation, PriceFeed, Reserve};

/// Withdraw posted share-token collateral, but only as long as the obligation
/// stays within its borrow limit afterwards. The post-withdraw allowed-borrow
/// value is simulated and the withdraw is rejected if the existing debt would
/// exceed it.
pub fn handle_withdraw_obligation_collateral(
    context: &mut Context<WithdrawObligationCollateral>,
    share_amount: u64,
) -> Result<()> {
    require!(share_amount > 0, LendingError::ZeroAmount);
    let slot = Clock::get()?.slot;

    context.accounts.obligation.require_refreshed()?;
    context.accounts.reserve.require_refreshed()?;
    let reserve = &context.accounts.reserve;
    let price_scaled = context.accounts.price_feed.price_scaled(slot)?;

    let obligation = &mut context.accounts.obligation;
    let index = obligation.find_collateral(*reserve.address())?;
    require!(
        obligation.deposits[index].deposited_shares >= share_amount,
        LendingError::WithdrawTooLarge
    );

    // Value of the collateral being removed, and the borrow power it backed.
    // Every step rounds UP: subtracting an over-estimate of the removed borrow
    // power guarantees the resulting allowance is never higher than a full
    // recompute would give, so independent flooring can't let a withdraw
    // squeak past the health check by a rounding sub-unit.
    let removed_liquidity = mul_div_ceil(
        share_amount as u128,
        reserve.total_liquidity()?,
        (reserve.share_mint_supply as u128).max(1),
    )?;
    let removed_liquidity =
        u64::try_from(removed_liquidity).map_err(|_| LendingError::MathOverflow)?;
    let removed_value = market_value(
        removed_liquidity,
        reserve.liquidity_decimals,
        price_scaled,
        Rounding::Up,
    )?;
    let removed_allowed = mul_div_ceil(
        removed_value,
        reserve.config.loan_to_value_bps as u128,
        BPS_DENOMINATOR,
    )?;
    // saturating_sub is correct here (and not balance math): the ceil-rounded
    // removal can exceed the floor-cached total by a sub-unit when withdrawing
    // everything, and zero remaining allowance is the conservative answer.
    let new_allowed_borrow_value = obligation
        .allowed_borrow_value
        .saturating_sub(removed_allowed);
    require!(
        obligation.borrowed_value <= new_allowed_borrow_value,
        LendingError::WithdrawTooLarge
    );

    // Effects.
    obligation.deposits[index].deposited_shares = obligation.deposits[index]
        .deposited_shares
        .checked_sub(share_amount)
        .ok_or(LendingError::MathOverflow)?;
    if obligation.deposits[index].deposited_shares == 0 {
        obligation.deposits.remove(index);
    }
    obligation.stale = true;

    let lending_market = obligation.lending_market;
    let owner = obligation.owner;
    let bump = [obligation.bump];
    let seeds: [&[u8]; 4] = [
        OBLIGATION_SEED,
        lending_market.as_ref(),
        owner.as_ref(),
        &bump,
    ];
    // `obligation` signs this CPI. It is a data account holding a live borrow on
    // its buffer, which the runtime would reject when the CPI borrows the same
    // account, so hand the borrow back across the call. `release_borrow`
    // flushes the pending writes, and `reacquire_borrow_mut` re-reads them.
    context.accounts.obligation.release_borrow()?;
    transfer_checked(
        CpiContext::new_with_signer(
            context.accounts.token_program.address(),
            TransferChecked {
                from: context.accounts.obligation_share_vault.cpi_handle_mut(),
                mint: context.accounts.share_mint.cpi_handle(),
                to: context.accounts.user_share.cpi_handle_mut(),
                authority: context.accounts.obligation.cpi_handle(),
            },
            &[&seeds],
        ),
        share_amount,
        context.accounts.share_mint.decimals(),
    )?;
    context.accounts.obligation.reacquire_borrow_mut()?;

    Ok(())
}

#[derive(Accounts)]
pub struct WithdrawObligationCollateral {
    #[account(mut)]
    pub obligation: BorshAccount<Obligation>,

    #[account(address = obligation.owner)]
    pub owner: Signer,

    #[account(
        constraint = reserve.lending_market == obligation.lending_market @ LendingError::MarketMismatch,
    )]
    pub reserve: BorshAccount<Reserve>,

    #[account(address = reserve.price_feed)]
    pub price_feed: BorshAccount<PriceFeed>,

    #[account(address = reserve.share_mint)]
    pub share_mint: InterfaceAccount<Mint>,

    #[account(
        mut,
        seeds = [OBLIGATION_SHARE_VAULT_SEED, reserve.address().as_ref(), obligation.address().as_ref()],
        bump,
        token::mint = share_mint,
        token::authority = obligation,
    )]
    pub obligation_share_vault: InterfaceAccount<TokenAccount>,

    #[account(mut)]
    pub user_share: InterfaceAccount<TokenAccount>,

    pub token_program: Interface<'static, TokenInterface>,
}
