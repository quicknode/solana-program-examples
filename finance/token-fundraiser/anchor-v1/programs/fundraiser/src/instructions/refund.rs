use anchor_lang::prelude::*;
use anchor_spl::token_interface::{
    transfer_checked, Mint, TokenAccount, TokenInterface, TransferChecked,
};

use crate::{
    state::{Contributor, Fundraiser},
    FundraiserError, SECONDS_TO_DAYS,
};

#[derive(Accounts)]
pub struct RefundAccountConstraints<'info> {
    #[account(mut)]
    pub contributor: Signer<'info>,

    pub maker: SystemAccount<'info>,

    pub mint_to_raise: InterfaceAccount<'info, Mint>,

    #[account(
        mut,
        has_one = mint_to_raise,
        seeds = [b"fundraiser", maker.key().as_ref()],
        bump = fundraiser.bump,
    )]
    pub fundraiser: Account<'info, Fundraiser>,

    #[account(
        mut,
        seeds = [b"contributor", fundraiser.key().as_ref(), contributor.key().as_ref()],
        bump = contributor_account.bump,
        close = contributor,
    )]
    pub contributor_account: Account<'info, Contributor>,

    #[account(
        mut,
        associated_token::mint = mint_to_raise,
        associated_token::authority = contributor,
        associated_token::token_program = token_program,
    )]
    pub contributor_ata: InterfaceAccount<'info, TokenAccount>,

    #[account(
        mut,
        associated_token::mint = mint_to_raise,
        associated_token::authority = fundraiser,
        associated_token::token_program = token_program,
    )]
    pub vault: InterfaceAccount<'info, TokenAccount>,

    pub token_program: Interface<'info, TokenInterface>,

    pub system_program: Program<'info, System>,
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

    // Transfer the funds from the vault back to the contributor. The vault is
    // owned by the fundraiser PDA, so the CPI is signed with its seeds.
    let cpi_accounts = TransferChecked {
        from: accounts.vault.to_account_info(),
        mint: accounts.mint_to_raise.to_account_info(),
        to: accounts.contributor_ata.to_account_info(),
        authority: accounts.fundraiser.to_account_info(),
    };
    let signer_seeds: [&[&[u8]]; 1] = [&[
        b"fundraiser".as_ref(),
        accounts.maker.to_account_info().key.as_ref(),
        &[accounts.fundraiser.bump],
    ]];
    let cpi_context = CpiContext::new_with_signer(
        accounts.token_program.key(),
        cpi_accounts,
        &signer_seeds,
    );
    transfer_checked(
        cpi_context,
        refund_amount,
        accounts.mint_to_raise.decimals,
    )?;

    Ok(())
}
