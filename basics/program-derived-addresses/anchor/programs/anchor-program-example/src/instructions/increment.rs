use crate::{state::PageVisits, PageVisitsError};
use anchor_lang::prelude::*;

#[derive(Accounts)]
pub struct IncrementPageVisitsAccountConstraints<'info> {
    user: SystemAccount<'info>,
    #[account(
        mut,
        seeds = [
            PageVisits::SEED_PREFIX,
            user.key().as_ref(),
        ],
        bump = page_visits.bump,
    )]
    page_visits: Account<'info, PageVisits>,
}

pub fn handle_increment_page_visits(
    context: Context<IncrementPageVisitsAccountConstraints>,
) -> Result<()> {
    let page_visits = &mut context.accounts.page_visits;
    page_visits.page_visits = page_visits
        .page_visits
        .checked_add(1)
        .ok_or(PageVisitsError::MathOverflow)?;
    Ok(())
}
