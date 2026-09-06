use anchor_lang::prelude::*;
use anchor_spl::token_interface::{
    transfer_checked, Mint, TokenAccount, TokenInterface, TransferChecked,
};

use crate::constants::AUTHORITY_SEED;
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
pub fn transfer_from_signer(
    token_program: &Interface<'static, TokenInterface>,
    from: &mut InterfaceAccount<TokenAccount>,
    mint: &InterfaceAccount<Mint>,
    to: &mut InterfaceAccount<TokenAccount>,
    signer: &Signer,
    amount: u64,
) -> Result<()> {
    transfer_checked(
        CpiContext::new(
            token_program.address(),
            TransferChecked {
                from: from.to_cpi_handle_mut(),
                mint: mint.to_cpi_handle(),
                to: to.to_cpi_handle_mut(),
                authority: signer.cpi_handle(),
            },
        ),
        amount,
        mint.decimals(),
    )
}

/// A transfer out of a vault, signed by the market's vault authority PDA.
/// Takes the market by reference for its address and authority bump, so the
/// caller must have finished mutating it (it has: effects come before CPIs).
pub fn transfer_from_vault(
    token_program: &Interface<'static, TokenInterface>,
    vault: &mut InterfaceAccount<TokenAccount>,
    mint: &InterfaceAccount<Mint>,
    to: &mut InterfaceAccount<TokenAccount>,
    market_authority: &UncheckedAccount,
    market: &BorshAccount<Market>,
    amount: u64,
) -> Result<()> {
    let market_key = market.address();
    let bump = [market.authority_bump];
    let authority_seeds: &[&[u8]] = &[AUTHORITY_SEED, market_key.as_ref(), &bump];
    transfer_checked(
        CpiContext::new_with_signer(
            token_program.address(),
            TransferChecked {
                from: vault.to_cpi_handle_mut(),
                mint: mint.to_cpi_handle(),
                to: to.to_cpi_handle_mut(),
                authority: market_authority.cpi_handle(),
            },
            &[authority_seeds],
        ),
        amount,
        mint.decimals(),
    )
}
