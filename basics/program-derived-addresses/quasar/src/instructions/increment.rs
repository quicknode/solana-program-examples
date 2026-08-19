use {
    crate::{error::PageVisitsError, state::PageVisits},
    quasar_lang::prelude::*,
};

/// Accounts for incrementing page visits.
/// The user account is needed to derive the PDA seeds for validation.
#[derive(Accounts)]
pub struct IncrementPageVisitsAccountConstraints {
    pub user: UncheckedAccount,
    #[account(mut)]
    pub page_visits: Account<PageVisits>,
}

#[inline(always)]
pub fn handle_increment_page_visits(
    accounts: &mut IncrementPageVisitsAccountConstraints,
) -> Result<(), ProgramError> {
    let current: u64 = accounts.page_visits.page_visits.into();
    let next = current
        .checked_add(1)
        .ok_or(PageVisitsError::MathOverflow)?;
    accounts.page_visits.page_visits = PodU64::from(next);
    Ok(())
}
