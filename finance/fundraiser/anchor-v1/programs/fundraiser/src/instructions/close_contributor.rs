use anchor_lang::prelude::*;

use crate::{
    state::{Contributor, Fundraiser},
    FundraiserError,
};

#[derive(Accounts)]
pub struct CloseContributorAccountConstraints<'info> {
    /// Not a signer: the rent goes to the contributor, whoever sends the
    /// transaction. So a maker can close every contributor account and then
    /// the fundraiser without waiting on any contributor.
    #[account(mut)]
    pub contributor: SystemAccount<'info>,

    #[account(
        mut,
        constraint = fundraiser.claimed @ FundraiserError::FundraiserNotClaimed,
    )]
    pub fundraiser: Account<'info, Fundraiser>,

    #[account(
        mut,
        seeds = [b"contributor", fundraiser.key().as_ref(), contributor.key().as_ref()],
        bump = contributor_account.bump,
        close = contributor,
    )]
    pub contributor_account: Account<'info, Contributor>,
}

/// Closes a contributor account once its fundraiser has been claimed,
/// returning the rent to the contributor.
///
/// `refund` closes contributor accounts on a failed raise. On a successful
/// one the contribution has been paid out to the maker, so the account only
/// holds rent, and `close_fundraiser` cannot run until every one of them is
/// closed. While the fundraiser is unclaimed the contribution can still be
/// refunded, so this handler refuses with `FundraiserNotClaimed`.
pub fn handle_close_contributor(accounts: &mut CloseContributorAccountConstraints) -> Result<()> {
    accounts.fundraiser.open_contributor_accounts = accounts
        .fundraiser
        .open_contributor_accounts
        .checked_sub(1)
        .ok_or(FundraiserError::MathOverflow)?;

    Ok(())
}
