use {
    crate::{
        error::FundraiserError,
        state::{Contributor, Fundraiser},
    },
    quasar_lang::prelude::*,
};

#[derive(Accounts)]
pub struct CloseContributorAccountConstraints {
    /// Not a signer: the rent goes to the contributor, whoever sends the
    /// transaction. So a maker can close every contributor account and then
    /// the fundraiser without waiting on any contributor.
    #[account(mut)]
    pub contributor: SystemAccount,

    /// The fundraiser this contributor account was written for. The
    /// contributor account's seeds bind it to this address, so no other
    /// fundraiser can be substituted.
    #[account(mut)]
    pub fundraiser: Account<Fundraiser>,

    #[account(
        mut,
        close(dest = contributor),
        address = Contributor::seeds(fundraiser.address(), contributor.address()),
    )]
    pub contributor_account: Account<Contributor>,
}

/// Closes a contributor account once its fundraiser has been claimed,
/// returning the rent to the contributor.
///
/// `refund` closes contributor accounts on a failed raise. On a successful
/// one the contribution has been paid out to the maker, so the account only
/// holds rent, and `close_fundraiser` cannot run until every one of them is
/// closed. While the fundraiser is unclaimed the contribution can still be
/// refunded, so this handler refuses with `FundraiserNotClaimed`.
#[inline(always)]
pub fn handle_close_contributor(
    accounts: &mut CloseContributorAccountConstraints,
) -> Result<(), ProgramError> {
    require!(
        bool::from(accounts.fundraiser.claimed),
        FundraiserError::FundraiserNotClaimed
    );

    let open_contributor_accounts: u32 = accounts.fundraiser.open_contributor_accounts.into();
    accounts.fundraiser.open_contributor_accounts = PodU32::from(
        open_contributor_accounts
            .checked_sub(1)
            .ok_or(FundraiserError::MathOverflow)?,
    );

    Ok(())
}
