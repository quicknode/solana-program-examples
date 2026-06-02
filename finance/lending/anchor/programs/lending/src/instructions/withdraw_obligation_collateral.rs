use anchor_lang::prelude::*;
use anchor_spl::token_interface::{
    transfer_checked, Mint, TokenAccount, TokenInterface, TransferChecked,
};

use crate::constants::{BPS_DENOMINATOR, OBLIGATION_SEED, OBLIGATION_SHARE_VAULT_SEED};
use crate::errors::LendingError;
use crate::math::{market_value, mul_div_floor, Rounding};
use crate::state::{Obligation, PriceFeed, Reserve};

/// Withdraw posted share-token collateral, but only as long as the obligation
/// stays within its borrow limit afterwards. The post-withdraw allowed-borrow
/// value is simulated and the withdraw is rejected if the existing debt would
/// exceed it.
pub fn handle_withdraw_obligation_collateral(
    context: Context<WithdrawObligationCollateral>,
    share_amount: u64,
) -> Result<()> {
    require!(share_amount > 0, LendingError::ZeroAmount);
    let slot = Clock::get()?.slot;

    context.accounts.obligation.require_refreshed()?;
    context.accounts.reserve.require_refreshed()?;
    let reserve = &context.accounts.reserve;
    let price_scaled = context.accounts.price_feed.price_scaled(slot)?;

    let obligation = &mut context.accounts.obligation;
    let index = obligation.find_collateral(reserve.key())?;
    require!(
        obligation.deposits[index].deposited_shares >= share_amount,
        LendingError::WithdrawTooLarge
    );

    // Value of the collateral being removed, and the borrow power it backed.
    let removed_liquidity = mul_div_floor(
        share_amount as u128,
        reserve.total_liquidity()?,
        (reserve.share_mint_supply as u128).max(1),
    )?;
    let removed_liquidity = u64::try_from(removed_liquidity).map_err(|_| LendingError::MathOverflow)?;
    let removed_value = market_value(
        removed_liquidity,
        reserve.liquidity_decimals,
        price_scaled,
        Rounding::Down,
    )?;
    let removed_allowed = mul_div_floor(
        removed_value,
        reserve.config.loan_to_value_bps as u128,
        BPS_DENOMINATOR,
    )?;
    let new_allowed_borrow_value = obligation
        .allowed_borrow_value
        .checked_sub(removed_allowed)
        .ok_or(LendingError::MathOverflow)?;
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
    transfer_checked(
        CpiContext::new_with_signer(
            context.accounts.token_program.key(),
            TransferChecked {
                from: context.accounts.obligation_share_vault.to_account_info(),
                mint: context.accounts.share_mint.to_account_info(),
                to: context.accounts.user_share.to_account_info(),
                authority: obligation.to_account_info(),
            },
            &[&seeds],
        ),
        share_amount,
        context.accounts.share_mint.decimals,
    )?;

    Ok(())
}

#[derive(Accounts)]
pub struct WithdrawObligationCollateral<'info> {
    #[account(mut, has_one = owner)]
    pub obligation: Account<'info, Obligation>,

    pub owner: Signer<'info>,

    #[account(has_one = share_mint, has_one = price_feed)]
    pub reserve: Account<'info, Reserve>,

    pub price_feed: Account<'info, PriceFeed>,

    pub share_mint: InterfaceAccount<'info, Mint>,

    #[account(
        mut,
        seeds = [OBLIGATION_SHARE_VAULT_SEED, reserve.key().as_ref(), obligation.key().as_ref()],
        bump,
        token::mint = share_mint,
        token::authority = obligation,
    )]
    pub obligation_share_vault: InterfaceAccount<'info, TokenAccount>,

    #[account(mut)]
    pub user_share: InterfaceAccount<'info, TokenAccount>,

    pub token_program: Interface<'info, TokenInterface>,
}
