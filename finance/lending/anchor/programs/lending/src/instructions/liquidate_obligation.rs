use anchor_lang::prelude::*;
use anchor_spl::token_interface::{
    transfer_checked, Mint, TokenAccount, TokenInterface, TransferChecked,
};

use crate::constants::{
    BPS_DENOMINATOR, FIXED_POINT_SCALE, OBLIGATION_SEED, OBLIGATION_SHARE_VAULT_SEED,
};
use crate::errors::LendingError;
use crate::math::{market_value, mul_div_ceil, mul_div_floor, value_to_amount, Rounding};
use crate::state::{Obligation, PriceFeed, Reserve};

/// Repay part of an unhealthy obligation's debt and seize collateral share
/// tokens worth the repayment plus the liquidation bonus.
///
/// The close factor caps how much of the borrow one call may repay; it comes
/// from the repay (borrow) reserve because it is a property of the debt being
/// closed. The liquidation bonus comes from the collateral reserve because it
/// prices the collateral being seized. If the requested repayment would seize
/// more collateral than the obligation holds, the call fails with
/// `LiquidationTooLarge` — silently capping the seizure would make the
/// liquidator pay full price for less collateral.
///
/// Self-liquidation (the owner liquidating their own position) is not blocked:
/// it is only possible while unhealthy and is economically pointless, matching
/// how Solend and Kamino behave.
pub fn handle_liquidate_obligation(
    context: Context<LiquidateObligation>,
    liquidity_amount: u64,
) -> Result<()> {
    require!(liquidity_amount > 0, LendingError::ZeroAmount);
    let slot = Clock::get()?.slot;

    context.accounts.obligation.require_refreshed()?;
    context.accounts.repay_reserve.require_refreshed()?;
    context.accounts.collateral_reserve.require_refreshed()?;

    let obligation = &context.accounts.obligation;
    let repay_reserve = &context.accounts.repay_reserve;
    let collateral_reserve = &context.accounts.collateral_reserve;

    require!(
        obligation.borrowed_value > obligation.unhealthy_borrow_value,
        LendingError::ObligationHealthy
    );

    let repay_price = context.accounts.repay_price_feed.price_scaled(slot)?;
    let collateral_price = context.accounts.collateral_price_feed.price_scaled(slot)?;

    let borrow_index = obligation.find_borrow(repay_reserve.key())?;
    let collateral_index = obligation.find_collateral(collateral_reserve.key())?;
    let borrowed_scaled = obligation.borrows[borrow_index].borrowed_scaled;
    let deposited_shares = obligation.deposits[collateral_index].deposited_shares;

    // How much debt this liquidation repays, capped by the close factor.
    let interest_index = repay_reserve.cumulative_borrow_rate_index;
    let debt_now = mul_div_ceil(borrowed_scaled, interest_index, FIXED_POINT_SCALE)?;
    let debt_now = u64::try_from(debt_now).map_err(|_| LendingError::MathOverflow)?;
    let max_repay = mul_div_floor(
        debt_now as u128,
        repay_reserve.config.close_factor_bps as u128,
        BPS_DENOMINATOR,
    )?;
    let repay = liquidity_amount.min(u64::try_from(max_repay).map_err(|_| LendingError::MathOverflow)?);
    require!(repay > 0, LendingError::ZeroAmount);

    // Collateral to seize: value of the repayment plus the bonus, converted into
    // the collateral token and then into share tokens. Every step rounds down,
    // toward the borrower, so the obligation is never over-seized by rounding.
    let repay_value = market_value(
        repay,
        repay_reserve.liquidity_decimals,
        repay_price,
        Rounding::Down,
    )?;
    let bonus_value = mul_div_floor(
        repay_value,
        collateral_reserve.config.liquidation_bonus_bps as u128,
        BPS_DENOMINATOR,
    )?;
    let seize_value = repay_value
        .checked_add(bonus_value)
        .ok_or(LendingError::MathOverflow)?;
    let seize_liquidity = value_to_amount(
        seize_value,
        collateral_reserve.liquidity_decimals,
        collateral_price,
        Rounding::Down,
    )?;
    let seize_shares = mul_div_floor(
        seize_liquidity as u128,
        collateral_reserve.share_mint_supply as u128,
        collateral_reserve.total_liquidity()?.max(1),
    )?;
    let seize_shares = u64::try_from(seize_shares).map_err(|_| LendingError::MathOverflow)?;
    require!(seize_shares > 0, LendingError::ZeroAmount);
    require!(
        seize_shares <= deposited_shares,
        LendingError::LiquidationTooLarge
    );

    let scaled_removed =
        mul_div_floor(repay as u128, FIXED_POINT_SCALE, interest_index)?.min(borrowed_scaled);

    // Effects: repay side.
    {
        let repay_reserve = &mut context.accounts.repay_reserve;
        repay_reserve.borrowed_amount_scaled = repay_reserve
            .borrowed_amount_scaled
            .checked_sub(scaled_removed)
            .ok_or(LendingError::MathOverflow)?;
        repay_reserve.available_liquidity = repay_reserve
            .available_liquidity
            .checked_add(repay)
            .ok_or(LendingError::MathOverflow)?;
    }

    // Effects: obligation debt and collateral.
    let (lending_market, owner, obligation_bump) = {
        let obligation = &mut context.accounts.obligation;
        obligation.borrows[borrow_index].borrowed_scaled = borrowed_scaled
            .checked_sub(scaled_removed)
            .ok_or(LendingError::MathOverflow)?;
        if obligation.borrows[borrow_index].borrowed_scaled == 0 {
            obligation.borrows.remove(borrow_index);
        }
        obligation.deposits[collateral_index].deposited_shares = deposited_shares
            .checked_sub(seize_shares)
            .ok_or(LendingError::MathOverflow)?;
        if obligation.deposits[collateral_index].deposited_shares == 0 {
            obligation.deposits.remove(collateral_index);
        }
        obligation.stale = true;
        (obligation.lending_market, obligation.owner, obligation.bump)
    };

    // Interactions: liquidator repays, then receives the seized share tokens.
    transfer_checked(
        CpiContext::new(
            context.accounts.token_program.key(),
            TransferChecked {
                from: context.accounts.liquidator_repay_source.to_account_info(),
                mint: context.accounts.repay_liquidity_mint.to_account_info(),
                to: context.accounts.repay_liquidity_vault.to_account_info(),
                authority: context.accounts.liquidator.to_account_info(),
            },
        ),
        repay,
        context.accounts.repay_reserve.liquidity_decimals,
    )?;

    let bump = [obligation_bump];
    let seeds: [&[u8]; 4] = [OBLIGATION_SEED, lending_market.as_ref(), owner.as_ref(), &bump];
    transfer_checked(
        CpiContext::new_with_signer(
            context.accounts.token_program.key(),
            TransferChecked {
                from: context.accounts.obligation_collateral_vault.to_account_info(),
                mint: context.accounts.collateral_share_mint.to_account_info(),
                to: context.accounts.liquidator_collateral_dest.to_account_info(),
                authority: context.accounts.obligation.to_account_info(),
            },
            &[&seeds],
        ),
        seize_shares,
        context.accounts.collateral_share_mint.decimals,
    )?;

    Ok(())
}

// Liquidation touches 13 accounts; every Account/InterfaceAccount is boxed so
// account deserialization happens on the heap and stays within the BPF stack frame.
#[derive(Accounts)]
pub struct LiquidateObligation<'info> {
    #[account(mut)]
    pub obligation: Box<Account<'info, Obligation>>,

    pub liquidator: Signer<'info>,

    #[account(
        mut,
        constraint = repay_reserve.lending_market == obligation.lending_market @ LendingError::MarketMismatch,
    )]
    pub repay_reserve: Box<Account<'info, Reserve>>,

    #[account(
        constraint = collateral_reserve.lending_market == obligation.lending_market @ LendingError::MarketMismatch,
    )]
    pub collateral_reserve: Box<Account<'info, Reserve>>,

    #[account(address = repay_reserve.price_feed)]
    pub repay_price_feed: Box<Account<'info, PriceFeed>>,

    #[account(address = collateral_reserve.price_feed)]
    pub collateral_price_feed: Box<Account<'info, PriceFeed>>,

    #[account(address = repay_reserve.liquidity_mint)]
    pub repay_liquidity_mint: Box<InterfaceAccount<'info, Mint>>,

    #[account(address = collateral_reserve.share_mint)]
    pub collateral_share_mint: Box<InterfaceAccount<'info, Mint>>,

    #[account(mut, address = repay_reserve.liquidity_vault)]
    pub repay_liquidity_vault: Box<InterfaceAccount<'info, TokenAccount>>,

    #[account(
        mut,
        seeds = [OBLIGATION_SHARE_VAULT_SEED, collateral_reserve.key().as_ref(), obligation.key().as_ref()],
        bump,
        token::mint = collateral_share_mint,
        token::authority = obligation,
    )]
    pub obligation_collateral_vault: Box<InterfaceAccount<'info, TokenAccount>>,

    #[account(mut)]
    pub liquidator_repay_source: Box<InterfaceAccount<'info, TokenAccount>>,

    #[account(mut)]
    pub liquidator_collateral_dest: Box<InterfaceAccount<'info, TokenAccount>>,

    pub token_program: Interface<'info, TokenInterface>,
}
