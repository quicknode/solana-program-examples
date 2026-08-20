use anchor_lang::prelude::*;

use crate::constants::OBLIGATION_SEED;
use crate::state::{LendingMarket, Obligation};

pub fn handle_initialize_obligation(context: &mut Context<InitializeObligation>) -> Result<()> {
    let obligation = &mut context.accounts.obligation;
    obligation.lending_market = *context.accounts.lending_market.address();
    obligation.owner = *context.accounts.owner.address();
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
pub struct InitializeObligation {
    pub lending_market: BorshAccount<LendingMarket>,

    #[account(
        init,
        payer = owner,
        space = Obligation::DISCRIMINATOR.len() + Obligation::INIT_SPACE,
        seeds = [OBLIGATION_SEED, lending_market.address().as_ref(), owner.address().as_ref()],
        bump,
    )]
    pub obligation: BorshAccount<Obligation>,

    #[account(mut)]
    pub owner: Signer,

    pub system_program: Program<System>,
}
