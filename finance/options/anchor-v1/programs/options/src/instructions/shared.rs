use anchor_lang::prelude::*;
use anchor_spl::token_interface::{
    transfer_checked, Mint, TokenAccount, TokenInterface, TransferChecked,
};

use crate::constants::MARKET_SEED;
use crate::errors::OptionsError;
use crate::state::Market;

/// The custody invariant, asserted after the math in every handler that moves
/// tokens: each vault covers what the market owes. `underlying_after` and
/// `quote_after` are the vault balances the handler's transfers will leave
/// behind, computed from the balances read before any CPI ran.
pub fn check_custody(market: &Market, underlying_after: u64, quote_after: u64) -> Result<()> {
    require!(
        underlying_after >= market.underlying_locked,
        OptionsError::CustodyInvariantViolated
    );
    let quote_owed = market
        .quote_locked
        .checked_add(market.fees_owed)
        .ok_or(OptionsError::MathOverflow)?;
    require!(
        quote_after >= quote_owed,
        OptionsError::CustodyInvariantViolated
    );
    Ok(())
}

/// A signer-authorized transfer into one of the vaults, or from one party to
/// another (the premium goes straight from buyer to writer).
pub fn transfer_from_signer<'info>(
    token_program: &Interface<'info, TokenInterface>,
    from: &mut InterfaceAccount<'info, TokenAccount>,
    mint: &InterfaceAccount<'info, Mint>,
    to: &mut InterfaceAccount<'info, TokenAccount>,
    signer: &Signer<'info>,
    amount: u64,
) -> Result<()> {
    transfer_checked(
        CpiContext::new(
            token_program.key(),
            TransferChecked {
                from: from.to_account_info(),
                mint: mint.to_account_info(),
                to: to.to_account_info(),
                authority: signer.to_account_info(),
            },
        ),
        amount,
        mint.decimals,
    )
}

/// A transfer out of a vault, signed by the market account, which is the
/// token authority of both vaults. Takes the market by reference for its
/// seeds and bump, so the caller must have finished mutating it (it has:
/// effects come before CPIs).
pub fn transfer_from_vault<'info>(
    token_program: &Interface<'info, TokenInterface>,
    vault: &mut InterfaceAccount<'info, TokenAccount>,
    mint: &InterfaceAccount<'info, Mint>,
    to: &mut InterfaceAccount<'info, TokenAccount>,
    market: &Account<'info, Market>,
    amount: u64,
) -> Result<()> {
    let bump = [market.bump];
    let market_seeds: &[&[u8]] = &[
        MARKET_SEED,
        market.underlying_mint.as_ref(),
        market.quote_mint.as_ref(),
        &bump,
    ];
    transfer_checked(
        CpiContext::new_with_signer(
            token_program.key(),
            TransferChecked {
                from: vault.to_account_info(),
                mint: mint.to_account_info(),
                to: to.to_account_info(),
                authority: market.to_account_info(),
            },
            &[market_seeds],
        ),
        amount,
        mint.decimals,
    )
}
