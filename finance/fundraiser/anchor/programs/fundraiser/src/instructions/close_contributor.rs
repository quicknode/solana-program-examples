use anchor_lang::prelude::*;

use crate::{state::Contributor, FundraiserError};

#[derive(Accounts)]
pub struct CloseContributorAccountConstraints {
    #[account(mut)]
    pub contributor: Signer,

    /// CHECK: the fundraiser this contributor account was written for. The
    /// contributor account's seeds bind it to this address, so no other
    /// fundraiser can be substituted. The constraint requires the account to
    /// be gone: a live fundraiser is owned by this program, and a closed one
    /// belongs to the system program again, whatever lamports it holds.
    #[account(
        constraint = !fundraiser.account().owned_by(&crate::ID) @ FundraiserError::FundraiserStillOpen,
    )]
    pub fundraiser: UncheckedAccount,

    #[account(
        mut,
        seeds = [b"contributor", fundraiser.address().as_ref(), contributor.address().as_ref()],
        bump = contributor_account.bump,
        close = contributor,
    )]
    pub contributor_account: BorshAccount<Contributor>,
}

/// Closes a contributor account once its fundraiser is gone, returning the
/// rent to the contributor.
///
/// A successful raise exits through `check_contributions`, which closes the
/// vault and the fundraiser but cannot reach the contributor accounts: there
/// is one per contributor and the claim carries none of them. Their other
/// closer, `refund`, runs only on a failed raise. Without this handler every
/// contributor to a successful raise would hold their rent in an account
/// nothing could close.
///
/// The one check is that the fundraiser account no longer exists, which is
/// the `constraint` above; the `close = contributor` constraint then returns
/// the rent. While the fundraiser exists the contribution is live, and
/// `refund` is the way to close it.
pub fn handle_close_contributor(_accounts: &mut CloseContributorAccountConstraints) -> Result<()> {
    Ok(())
}
