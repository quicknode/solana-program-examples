use anchor_lang::prelude::*;
use anchor_spl::token;
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
    context: &mut Context<LiquidateObligation>,
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

    let borrow_index = obligation.find_borrow(*repay_reserve.address())?;
    let collateral_index = obligation.find_collateral(*collateral_reserve.address())?;
    let borrowed_principal = obligation.borrows[borrow_index].borrowed_principal;
    let deposited_shares = obligation.deposits[collateral_index].deposited_shares;

    // How much debt this liquidation repays, capped by the close factor.
    let accumulation_factor = repay_reserve.borrow_accumulation_factor;
    let debt_now = mul_div_ceil(borrowed_principal, accumulation_factor, FIXED_POINT_SCALE)?;
    let debt_now = u64::try_from(debt_now).map_err(|_| LendingError::MathOverflow)?;
    let max_repay = mul_div_floor(
        debt_now as u128,
        repay_reserve.config.close_factor_bps as u128,
        BPS_DENOMINATOR,
    )?;
    let repay =
        liquidity_amount.min(u64::try_from(max_repay).map_err(|_| LendingError::MathOverflow)?);
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

    let scaled_removed = mul_div_floor(repay as u128, FIXED_POINT_SCALE, accumulation_factor)?
        .min(borrowed_principal);

    // Effects: repay side.
    {
        let repay_reserve = &mut context.accounts.repay_reserve;
        repay_reserve.borrowed_principal = repay_reserve
            .borrowed_principal
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
        obligation.borrows[borrow_index].borrowed_principal = borrowed_principal
            .checked_sub(scaled_removed)
            .ok_or(LendingError::MathOverflow)?;
        if obligation.borrows[borrow_index].borrowed_principal == 0 {
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
    //
    // These accounts are `Box`ed, and `Box`'s `AnchorAccount` impl does not
    // override `cpi_handle_mut`, so that call would build a handle without
    // releasing the wrapper's data borrow and the CPI would be rejected with
    // `AccountBorrowFailed`. `to_cpi_handle_mut` forwards to the inner type.
    transfer_checked(
        CpiContext::new(
            context.accounts.token_program.address(),
            TransferChecked {
                from: context.accounts.liquidator_repay_source.to_cpi_handle_mut(),
                mint: context.accounts.repay_liquidity_mint.to_cpi_handle(),
                to: context.accounts.repay_liquidity_vault.to_cpi_handle_mut(),
                authority: context.accounts.liquidator.cpi_handle(),
            },
        ),
        repay,
        context.accounts.repay_reserve.liquidity_decimals,
    )?;

    let bump = [obligation_bump];
    let seeds: [&[u8]; 4] = [
        OBLIGATION_SEED,
        lending_market.as_ref(),
        owner.as_ref(),
        &bump,
    ];
    // `obligation` signs this CPI. It is a data account holding a live borrow on
    // its buffer, which the runtime would reject when the CPI borrows the same
    // account — so hand the borrow back across the call. `release_borrow`
    // flushes the pending writes, and `reacquire_borrow_mut` re-reads them.
    context.accounts.obligation.release_borrow()?;
    transfer_checked(
        CpiContext::new_with_signer(
            context.accounts.token_program.address(),
            TransferChecked {
                from: context
                    .accounts
                    .obligation_collateral_vault
                    .to_cpi_handle_mut(),
                mint: context.accounts.collateral_share_mint.to_cpi_handle(),
                to: context.accounts.liquidator_collateral_dest.to_cpi_handle_mut(),
                authority: context.accounts.obligation.cpi_handle(),
            },
            &[&seeds],
        ),
        seize_shares,
        context.accounts.collateral_share_mint.decimals(),
    )?;
    context.accounts.obligation.reacquire_borrow_mut()?;

    Ok(())
}

// Liquidation touches 13 accounts; every Account/InterfaceAccount is boxed so
// account deserialization happens on the heap and stays within the BPF stack frame.
#[derive(Accounts)]
pub struct LiquidateObligation {
    #[account(mut)]
    pub obligation: Box<BorshAccount<Obligation>>,

    pub liquidator: Signer,

    #[account(
        mut,
        constraint = repay_reserve.lending_market == obligation.lending_market @ LendingError::MarketMismatch,
    )]
    pub repay_reserve: Box<BorshAccount<Reserve>>,

    #[account(
        constraint = collateral_reserve.lending_market == obligation.lending_market @ LendingError::MarketMismatch,
    )]
    pub collateral_reserve: Box<BorshAccount<Reserve>>,

    #[account(address = repay_reserve.price_feed)]
    pub repay_price_feed: Box<BorshAccount<PriceFeed>>,

    #[account(address = collateral_reserve.price_feed)]
    pub collateral_price_feed: Box<BorshAccount<PriceFeed>>,

    #[account(address = repay_reserve.liquidity_mint)]
    pub repay_liquidity_mint: Box<InterfaceAccount<Mint>>,

    #[account(address = collateral_reserve.share_mint)]
    pub collateral_share_mint: Box<InterfaceAccount<Mint>>,

    #[account(mut, address = repay_reserve.liquidity_vault)]
    pub repay_liquidity_vault: Box<InterfaceAccount<TokenAccount>>,

    #[account(
        mut,
        seeds = [OBLIGATION_SHARE_VAULT_SEED, collateral_reserve.address().as_ref(), obligation.address().as_ref()],
        bump,
        token::mint = collateral_share_mint,
        token::authority = obligation,
    )]
    pub obligation_collateral_vault: Box<InterfaceAccount<TokenAccount>>,

    #[account(mut)]
    pub liquidator_repay_source: Box<InterfaceAccount<TokenAccount>>,

    #[account(mut)]
    pub liquidator_collateral_dest: Box<InterfaceAccount<TokenAccount>>,

    pub token_program: Interface<'static, TokenInterface>,
}
