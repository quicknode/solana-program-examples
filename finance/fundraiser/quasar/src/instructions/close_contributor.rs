use {
    crate::{error::FundraiserError, state::Contributor},
    quasar_lang::prelude::*,
};

#[derive(Accounts)]
pub struct CloseContributorAccountConstraints {
    #[account(mut)]
    pub contributor: Signer,

    /// The fundraiser this contributor account was written for. The
    /// contributor account's seeds bind it to this address, so no other
    /// fundraiser can be substituted. The constraint requires the account to
    /// be gone: a live fundraiser is owned by this program, and a closed one
    /// belongs to the system program again, whatever lamports it holds.
    #[account(
        constraints(fundraiser.to_account_view().owner() != &crate::ID)
            @ FundraiserError::FundraiserStillOpen
    )]
    pub fundraiser: UncheckedAccount,

    #[account(
        mut,
        close(dest = contributor),
        address = Contributor::seeds(fundraiser.address(), contributor.address()),
    )]
    pub contributor_account: Account<Contributor>,
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
/// the `constraints` above; the `close(dest = contributor)` constraint then
/// returns the rent. While the fundraiser exists the contribution is live,
/// and `refund` is the way to close it.
#[inline(always)]
pub fn handle_close_contributor(
    _accounts: &mut CloseContributorAccountConstraints,
) -> Result<(), ProgramError> {
    Ok(())
}
