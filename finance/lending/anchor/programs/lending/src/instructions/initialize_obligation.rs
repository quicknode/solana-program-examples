use anchor_lang::prelude::*;

use crate::constants::OBLIGATION_SEED;
use crate::state::{LendingMarket, Obligation};

pub fn handle_initialize_obligation(context: Context<InitializeObligation>) -> Result<()> {
    let obligation = &mut context.accounts.obligation;
    obligation.lending_market = context.accounts.lending_market.key();
    obligation.owner = context.accounts.owner.key();
    obligation.last_update_slot = Clock::get()?.slot;
    // Stale until the first refresh; an empty obligation has nothing to value yet.
    obligation.stale = true;
    obligation.deposited_value = 0;
    obligation.borrowed_value = 0;
    obligation.allowed_borrow_value = 0;
    obligation.unhealthy_borrow_value = 0;
    obligation.deposits = Vec::new();
    obligation.borrows = Vec::new();
    obligation.bump = context.bumps.obligation;
    Ok(())
}

#[derive(Accounts)]
pub struct InitializeObligation<'info> {
    pub lending_market: Account<'info, LendingMarket>,

    #[account(
        init,
        payer = owner,
        space = Obligation::DISCRIMINATOR.len() + Obligation::INIT_SPACE,
        seeds = [OBLIGATION_SEED, lending_market.key().as_ref(), owner.key().as_ref()],
        bump,
    )]
    pub obligation: Account<'info, Obligation>,

    #[account(mut)]
    pub owner: Signer<'info>,

    pub system_program: Program<'info, System>,
}
