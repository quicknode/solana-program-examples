use anchor_lang::prelude::*;
use anchor_spl::token_interface::{
    transfer_checked, Mint, TokenAccount, TokenInterface, TransferChecked,
};

use crate::{
    state::{Contribution, Fundraiser},
    FundraiserError, SECONDS_TO_DAYS,
};

#[derive(Accounts)]
pub struct ContributeAccountConstraints {
    #[account(mut)]
    pub contributor: Signer,

    #[account(address = fundraiser.mint_to_raise)]
    pub mint_to_raise: InterfaceAccount<Mint>,

    #[account(
        mut,
        seeds = [b"fundraiser".as_ref(), fundraiser.maker.as_ref()],
        bump = fundraiser.bump,
    )]
    pub fundraiser: BorshAccount<Fundraiser>,

    #[account(
        init_if_needed,
        payer = contributor,
        seeds = [b"contribution", fundraiser.address().as_ref(), contributor.address().as_ref()],
        bump,
        space = Contribution::DISCRIMINATOR.len() + Contribution::INIT_SPACE,
    )]
    pub contribution: BorshAccount<Contribution>,

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

    // A claimed fundraiser has paid its vault out to the maker, so a later
    // contribution would go to the maker with no refund path.
    require!(
        !accounts.fundraiser.claimed,
        FundraiserError::FundraiserClaimed
    );

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

    let cumulative_contribution = accounts
        .contribution
        .amount
        .checked_add(amount)
        .ok_or(FundraiserError::MathOverflow)?;

    accounts.fundraiser.current_amount = accounts
        .fundraiser
        .current_amount
        .checked_add(amount)
        .ok_or(FundraiserError::MathOverflow)?;
    accounts.contribution.amount = cumulative_contribution;

    // On first init (init_if_needed only runs the init branch once; the
    // stored bump is zero until set), save the contributor PDA bump and count
    // the new contribution account against the fundraiser.
    if accounts.contribution.bump == 0 {
        accounts.contribution.bump = bumps.contribution;
        accounts.fundraiser.open_contributions = accounts
            .fundraiser
            .open_contributions
            .checked_add(1)
            .ok_or(FundraiserError::MathOverflow)?;
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
