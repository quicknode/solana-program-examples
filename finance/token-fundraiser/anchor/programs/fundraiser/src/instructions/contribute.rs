use anchor_lang::prelude::*;
use anchor_spl::token_interface::{
    transfer_checked, Mint, TokenAccount, TokenInterface, TransferChecked,
};

use crate::{
    state::{Contributor, Fundraiser},
    FundraiserError, MAX_CONTRIBUTION_PERCENTAGE, PERCENTAGE_SCALER, SECONDS_TO_DAYS,
};

#[derive(Accounts)]
pub struct ContributeAccountConstraints {
    #[account(mut)]
    pub contributor: Signer,

    pub mint_to_raise: InterfaceAccount<Mint>,

    #[account(
        mut,
        has_one = mint_to_raise,
        seeds = [b"fundraiser".as_ref(), fundraiser.maker.as_ref()],
        bump = fundraiser.bump,
    )]
    pub fundraiser: BorshAccount<Fundraiser>,

    #[account(
        init_if_needed,
        payer = contributor,
        seeds = [b"contributor", fundraiser.address().as_ref(), contributor.address().as_ref()],
        bump,
        space = Contributor::DISCRIMINATOR.len() + Contributor::INIT_SPACE,
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

/// Caps a single contributor at MAX_CONTRIBUTION_PERCENTAGE percent of the
/// target. Multiplies in u128 so the product cannot overflow u64.
fn calculate_max_contribution(amount_to_raise: u64) -> Result<u64> {
    (amount_to_raise as u128)
        .checked_mul(MAX_CONTRIBUTION_PERCENTAGE as u128)
        .ok_or(FundraiserError::MathOverflow)?
        .checked_div(PERCENTAGE_SCALER as u128)
        .ok_or(FundraiserError::MathOverflow)?
        .try_into()
        .map_err(|_| error!(FundraiserError::MathOverflow))
}

pub fn handle_contribute(
    accounts: &mut ContributeAccountConstraints,
    amount: u64,
    bumps: &ContributeAccountConstraintsBumps,
) -> Result<()> {
    // The minimum contribution is one major unit, which is 10^decimals minor units.
    let one_major_unit = 10_u64
        .checked_pow(accounts.mint_to_raise.decimals() as u32)
        .ok_or(FundraiserError::MathOverflow)?;
    require!(
        amount >= one_major_unit,
        FundraiserError::ContributionTooSmall
    );

    let max_contribution = calculate_max_contribution(accounts.fundraiser.amount_to_raise)?;
    require!(
        amount <= max_contribution,
        FundraiserError::ContributionTooBig
    );

    // Contributions are allowed while elapsed_days < duration.
    let current_time = Clock::get()?.unix_timestamp;
    let elapsed_days = current_time
        .checked_sub(accounts.fundraiser.time_started)
        .ok_or(FundraiserError::MathOverflow)?
        .checked_div(SECONDS_TO_DAYS)
        .ok_or(FundraiserError::MathOverflow)?;
    require!(
        elapsed_days < accounts.fundraiser.duration as i64,
        FundraiserError::FundraiserEnded
    );

    // The contributor's cumulative total must also stay within the cap.
    let cumulative_contribution = accounts
        .contributor_account
        .amount
        .checked_add(amount)
        .ok_or(FundraiserError::MathOverflow)?;
    require!(
        cumulative_contribution <= max_contribution,
        FundraiserError::MaximumContributionsReached
    );

    // Checks-effects-interactions: update state before the transfer CPI.
    accounts.fundraiser.current_amount = accounts
        .fundraiser
        .current_amount
        .checked_add(amount)
        .ok_or(FundraiserError::MathOverflow)?;
    accounts.contributor_account.amount = cumulative_contribution;

    // Save the contributor PDA bump on first init (init_if_needed only
    // runs the init branch once; stored bump is zero until set).
    if accounts.contributor_account.bump == 0 {
        accounts.contributor_account.bump = bumps.contributor_account;
    }

    // Transfer the funds from the contributor to the vault.
    let cpi_accounts = TransferChecked {
        from: accounts.contributor_ata.cpi_handle_mut(),
        mint: accounts.mint_to_raise.cpi_handle(),
        to: accounts.vault.cpi_handle_mut(),
        authority: accounts.contributor.cpi_handle(),
    };
    let cpi_context = CpiContext::new(accounts.token_program.address(), cpi_accounts);
    transfer_checked(cpi_context, amount, accounts.mint_to_raise.decimals())?;

    Ok(())
}
