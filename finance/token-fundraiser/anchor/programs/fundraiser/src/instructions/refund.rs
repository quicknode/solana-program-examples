use anchor_lang::prelude::*;
use anchor_spl::token_interface::{
    transfer_checked, Mint, TokenAccount, TokenInterface, TransferChecked,
};

use crate::{
    state::{Contributor, Fundraiser},
    FundraiserError, SECONDS_TO_DAYS,
};

#[derive(Accounts)]
pub struct RefundAccountConstraints {
    #[account(mut)]
    pub contributor: Signer,

    pub maker: SystemAccount,

    #[account(address = fundraiser.mint_to_raise)]
    pub mint_to_raise: InterfaceAccount<Mint>,

    #[account(
        mut,
        seeds = [b"fundraiser", maker.address().as_ref()],
        bump = fundraiser.bump,
    )]
    pub fundraiser: BorshAccount<Fundraiser>,

    #[account(
        mut,
        seeds = [b"contributor", fundraiser.address().as_ref(), contributor.address().as_ref()],
        bump = contributor_account.bump,
        close = contributor,
    )]
    pub contributor_account: BorshAccount<Contributor>,

    #[account(
        mut,
        associated_token::mint = mint_to_raise,
        associated_token::authority = contributor,
        associated_token::token_program = token_program,
    )]
    pub contributor_ata: InterfaceAccount<TokenAccount>,

    #[account(
        mut,
        associated_token::mint = mint_to_raise,
        associated_token::authority = fundraiser,
        associated_token::token_program = token_program,
    )]
    pub vault: InterfaceAccount<TokenAccount>,

    pub token_program: Interface<'static, TokenInterface>,

    pub system_program: Program<System>,
}

pub fn handle_refund(accounts: &mut RefundAccountConstraints) -> Result<()> {
    // Refunds are allowed only after the fundraiser has ended:
    // elapsed_days >= duration.
    let current_time = Clock::get()?.unix_timestamp;
    let elapsed_days = current_time
        .checked_sub(accounts.fundraiser.time_started)
        .ok_or(FundraiserError::MathOverflow)?
        .checked_div(SECONDS_TO_DAYS)
        .ok_or(FundraiserError::MathOverflow)?;
    require!(
        elapsed_days >= accounts.fundraiser.duration as i64,
        FundraiserError::FundraiserNotEnded
    );

    // Refunds are allowed only when the target was not met. Compare the
    // state-tracked total, not the vault balance, so tokens donated directly
    // to the vault cannot block refunds.
    require!(
        accounts.fundraiser.current_amount < accounts.fundraiser.amount_to_raise,
        FundraiserError::TargetMet
    );

    // Checks-effects-interactions: update state before the transfer CPI.
    let refund_amount = accounts.contributor_account.amount;
    accounts.fundraiser.current_amount = accounts
        .fundraiser
        .current_amount
        .checked_sub(refund_amount)
        .ok_or(FundraiserError::MathOverflow)?;
    accounts.contributor_account.amount = 0;

    // Read these before any CPI handle below takes its borrow. `maker` is a
    // read-only account here, so asking it for a writable handle would panic.
    let maker_address = *accounts.maker.address();
    let mint_decimals = accounts.mint_to_raise.decimals();
    let fundraiser_bump = accounts.fundraiser.bump;

    // `fundraiser` signs the transfer. It is a data account holding a live
    // borrow on its buffer, so release it across the CPI and take it back after.
    accounts.fundraiser.release_borrow()?;
    let fundraiser_view = *accounts.fundraiser.account();

    // Transfer the funds from the vault back to the contributor. The vault is
    // owned by the fundraiser PDA, so the CPI is signed with its seeds.
    let cpi_accounts = TransferChecked {
        from: accounts.vault.cpi_handle_mut(),
        mint: accounts.mint_to_raise.cpi_handle(),
        to: accounts.contributor_ata.cpi_handle_mut(),
        authority: CpiHandle::readonly(&fundraiser_view),
    };
    let signer_seeds: [&[&[u8]]; 1] = [&[
        b"fundraiser".as_ref(),
        maker_address.as_ref(),
        &[fundraiser_bump],
    ]];
    let cpi_context = CpiContext::new_with_signer(
        accounts.token_program.address(),
        cpi_accounts,
        &signer_seeds,
    );
    transfer_checked(cpi_context, refund_amount, mint_decimals)?;

    // Take the borrow back before the derive's exit path touches it again.
    accounts.fundraiser.reacquire_borrow_mut()?;

    Ok(())
}
