use crate::state::PageVisits;
use anchor_lang::prelude::*;

#[derive(Accounts)]
pub struct CreatePageVisitsAccountConstraints {
    #[account(mut)]
    pub payer: Signer,

    #[account(
        init,
        space = PageVisits::DISCRIMINATOR.len() + PageVisits::INIT_SPACE,
        payer = payer,
        seeds = [
            PageVisits::SEED_PREFIX,
            payer.address().as_ref(),
        ],
        bump,
    )]
    pub page_visits: Account<PageVisits>,
    pub system_program: Program<System>,
}

pub fn handle_create_page_visits(
    context: &mut Context<CreatePageVisitsAccountConstraints>,
) -> Result<()> {
    *context.accounts.page_visits = PageVisits {
        page_visits: 0,
        bump: context.bumps.page_visits,
        _padding: [0; 3],
    };

    Ok(())
}
