use anchor_lang::prelude::*;
use anchor_spl::token;
use anchor_spl::token_interface::{
    close_account, transfer_checked, CloseAccount, Mint, TokenAccount, TokenInterface,
    TransferChecked,
};

use crate::constants::{BPS_DENOMINATOR, OBLIGATION_SEED, OBLIGATION_SHARE_VAULT_SEED};
use crate::errors::LendingError;
use crate::math::{market_value, mul_div_ceil, Rounding};
use crate::state::{Obligation, PriceFeed, Reserve};

/// Withdraw posted share-token collateral.
///
/// With debt outstanding this is a health-dependent action: the obligation
/// and the reserve must have been refreshed in this transaction, the price
/// must be fresh, and the obligation must stay within its borrow limit
/// afterwards. The post-withdraw allowed-borrow value is simulated and the
/// withdraw is rejected if the existing debt would exceed it.
///
/// With no borrows at all the collateral backs nothing, so none of that
/// applies: no refresh, no price, no health check, and the whole deposit can
/// come out whatever the price feed is doing. A borrower who owes nothing
/// must never be locked in by a stale or silent oracle. `borrows` is empty
/// exactly when the debt is zero, since a borrow entry is removed when its
/// last unit is repaid. Either way the obligation is marked stale, so its
/// cached values are recomputed before the next health-dependent action.
///
/// A withdrawal that takes the last share of a reserve removes the deposit
/// entry and closes that reserve's collateral vault, returning its rent to the
/// owner, who paid it when `deposit_obligation_collateral` created the vault
/// (`init_if_needed` recreates it on a later deposit). The whole vault balance
/// goes to the owner first, so share tokens someone sent straight to the vault
/// cannot keep it open or block the withdrawal.
pub fn handle_withdraw_obligation_collateral(
    context: &mut Context<WithdrawObligationCollateral>,
    share_amount: u64,
) -> Result<()> {
    require!(share_amount > 0, LendingError::ZeroAmount);

    let reserve_key = *context.accounts.reserve.address();
    let index = context.accounts.obligation.find_collateral(reserve_key)?;
    require!(
        context.accounts.obligation.deposits[index].deposited_shares >= share_amount,
        LendingError::WithdrawTooLarge
    );

    if !context.accounts.obligation.borrows.is_empty() {
        require_within_borrow_limit_after(context, share_amount)?;
    }

    let obligation = &mut context.accounts.obligation;

    // Effects.
    obligation.deposits[index].deposited_shares = obligation.deposits[index]
        .deposited_shares
        .checked_sub(share_amount)
        .ok_or(LendingError::MathOverflow)?;
    let empties_vault = obligation.deposits[index].deposited_shares == 0;
    if empties_vault {
        obligation.deposits.remove(index);
    }
    obligation.stale = true;
    // Emptying the entry sweeps the vault, donations included, so it can close.
    let transfer_amount = if empties_vault {
        context.accounts.obligation_share_vault.amount()
    } else {
        share_amount
    };

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
        transfer_amount,
        context.accounts.share_mint.decimals(),
    )?;
    if empties_vault {
        close_account(CpiContext::new_with_signer(
            context.accounts.token_program.address(),
            CloseAccount {
                account: context.accounts.obligation_share_vault.cpi_handle_mut(),
                destination: context.accounts.owner.cpi_handle_mut(),
                authority: context.accounts.obligation.cpi_handle(),
            },
            &[&seeds],
        ))?;
    }
    context.accounts.obligation.reacquire_borrow_mut()?;

    Ok(())
}

/// The health check for a withdrawal from an obligation with debt: the
/// obligation and reserve must be refreshed this transaction, the price fresh,
/// and the debt must still fit under the borrow limit once `share_amount`
/// shares of `reserve` are gone.
fn require_within_borrow_limit_after(
    context: &Context<WithdrawObligationCollateral>,
    share_amount: u64,
) -> Result<()> {
    let slot = Clock::get()?.slot;
    let obligation = &context.accounts.obligation;
    let reserve = &context.accounts.reserve;
    obligation.require_refreshed()?;
    reserve.require_refreshed()?;
    let price_scaled = context
        .accounts
        .price_feed
        .price_scaled(slot, reserve.config.max_confidence_bps)?;

    // Value of the collateral being removed, and the borrow power it backed.
    // Every step rounds UP: subtracting an over-estimate of the removed borrow
    // power guarantees the resulting allowance is never higher than a full
    // recompute would give, so independent flooring can't let a withdraw
    // squeak past the health check by a rounding sub-unit.
    let removed_liquidity = mul_div_ceil(
        share_amount as u128,
        reserve.total_liquidity()?,
        reserve.total_shares()?,
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
    Ok(())
}

#[derive(Accounts)]
pub struct WithdrawObligationCollateral {
    #[account(mut)]
    pub obligation: BorshAccount<Obligation>,

    /// Writable: a withdrawal that empties the vault closes it, rent to here.
    #[account(mut, address = obligation.owner)]
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
