use {
    crate::state::{PageVisits, PageVisitsInner},
    quasar_lang::prelude::*,
};

/// Accounts for creating a new page visits counter.
/// The counter is derived as a PDA from ["page_visits", payer] seeds.
#[derive(Accounts)]
pub struct CreatePageVisits {
    #[account(mut)]
    pub payer: Signer,
    #[account(mut, init, payer = payer, address = PageVisits::seeds(payer.address()))]
    pub page_visits: Account<PageVisits>,
    pub system_program: Program<SystemProgram>,
}

#[inline(always)]
pub fn handle_create_page_visits(accounts: &mut CreatePageVisits) -> Result<(), ProgramError> {
    accounts.page_visits.set_inner(PageVisitsInner { page_visits: 0 });
    Ok(())
}
