use crate::{state::PageVisits, PageVisitsError};
use anchor_lang::prelude::*;

#[derive(Accounts)]
pub struct IncrementPageVisitsAccountConstraints {
    user: SystemAccount,
    #[account(
        mut,
        seeds = [
            PageVisits::SEED_PREFIX,
            user.address().as_ref(),
        ],
        bump = page_visits.bump,
    )]
    page_visits: Account<PageVisits>,
}

pub fn handle_increment_page_visits(
    context: &mut Context<IncrementPageVisitsAccountConstraints>,
) -> Result<()> {
    let page_visits = &mut context.accounts.page_visits;
    page_visits.page_visits = page_visits
        .page_visits
        .checked_add(1)
        .ok_or(PageVisitsError::MathOverflow)?;
    Ok(())
}
