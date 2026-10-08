use crate::state::PageVisits;
use anchor_lang::prelude::*;

#[derive(Accounts)]
pub struct CreatePageVisitsAccountConstraints<'info> {
    #[account(mut)]
    pub payer: Signer<'info>,

    #[account(
        init,
        space = PageVisits::DISCRIMINATOR.len() + PageVisits::INIT_SPACE,
        payer = payer,
        seeds = [
            PageVisits::SEED_PREFIX,
            payer.key().as_ref(),
        ],
        bump,
    )]
    pub page_visits: Account<'info, PageVisits>,
    pub system_program: Program<'info, System>,
}

pub fn handle_create_page_visits(
    context: Context<CreatePageVisitsAccountConstraints>,
) -> Result<()> {
    *context.accounts.page_visits = PageVisits {
        page_visits: 0,
        bump: context.bumps.page_visits,
    };

    Ok(())
}
