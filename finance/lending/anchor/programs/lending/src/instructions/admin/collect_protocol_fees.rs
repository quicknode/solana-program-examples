use anchor_lang::prelude::*;
use anchor_spl::token_interface::{
    transfer_checked, Mint, TokenAccount, TokenInterface, TransferChecked,
};

use crate::errors::LendingError;
use crate::state::{reserve_signer_seeds, LendingMarket, Reserve};

/// Withdraw the protocol fees accrued in a reserve to the market owner. This is
/// how the owner earns: `reserve_factor_bps` of every interest accrual is set
/// aside in `accumulated_protocol_fees` (never credited to suppliers), and this
/// handler pays it out, capped by the liquidity actually sitting in the vault.
pub fn handle_collect_protocol_fees(context: &mut Context<CollectProtocolFees>) -> Result<()> {
    context.accounts.reserve.require_refreshed()?;

    let reserve = &mut context.accounts.reserve;
    // Fees are a claim on liquidity; only what is currently un-borrowed can be paid
    // out right now. Any remainder stays owed until borrowers repay.
    let amount = reserve
        .accumulated_protocol_fees
        .min(reserve.available_liquidity);
    require!(amount > 0, LendingError::NothingToCollect);

    reserve.accumulated_protocol_fees = reserve
        .accumulated_protocol_fees
        .checked_sub(amount)
        .ok_or(LendingError::MathOverflow)?;
    reserve.available_liquidity = reserve
        .available_liquidity
        .checked_sub(amount)
        .ok_or(LendingError::MathOverflow)?;

    let bump = [reserve.bump];
    let seeds = reserve_signer_seeds(&reserve.lending_market, &reserve.liquidity_mint, &bump);
    transfer_checked(
        CpiContext::new_with_signer(
            context.accounts.token_program.address(),
            TransferChecked {
                from: context.accounts.liquidity_vault.cpi_handle_mut(),
                mint: context.accounts.liquidity_mint.cpi_handle(),
                to: context.accounts.owner_liquidity.cpi_handle_mut(),
                authority: reserve.cpi_handle(),
            },
            &[&seeds],
        ),
        amount,
        reserve.liquidity_decimals,
    )?;

    Ok(())
}

#[derive(Accounts)]
pub struct CollectProtocolFees {
    // Identified by the reserve's `has_one = lending_market`; we only prove the
    // signer owns it.
    #[account(has_one = owner)]
    pub lending_market: BorshAccount<LendingMarket>,

    #[account(mut)]
    pub owner: Signer,

    #[account(
        mut,
        has_one = lending_market,
        has_one = liquidity_mint,
        has_one = liquidity_vault,
    )]
    pub reserve: BorshAccount<Reserve>,

    pub liquidity_mint: InterfaceAccount<Mint>,

    #[account(mut)]
    pub liquidity_vault: InterfaceAccount<TokenAccount>,

    #[account(mut)]
    pub owner_liquidity: InterfaceAccount<TokenAccount>,

    pub token_program: Interface<'static, TokenInterface>,
}
