use anchor_lang::prelude::*;

use crate::state::Event;

use crate::{error::BettingError, Config, EventStatus, Outcome};

pub const MAX_LABEL_LEN: usize = 64;

#[derive(Accounts)]
pub struct AddOutcomeAccountConstraints {
    #[account(mut, address = config.admin @ BettingError::Unauthorized)]
    pub admin: Signer,

    #[account(seeds = [b"config"],
        bump = config.bump)]
    pub config: BorshAccount<Config>,

    #[account(
        mut,
        seeds = [b"event", event.event_id.to_le_bytes()],
        bump = event.bump,
    )]
    pub event: BorshAccount<Event>,

    #[account(
        init,
        payer = admin,
        space = Outcome::DISCRIMINATOR.len() + Outcome::INIT_SPACE,
        seeds = [b"outcome", event.address().as_ref(), &[event.outcome_count]],
        bump
    )]
    pub outcome: BorshAccount<Outcome>,

    pub system_program: Program<System>,
}

pub fn handle_add_outcome(
    context: &mut Context<AddOutcomeAccountConstraints>,
    label: String,
) -> Result<()> {
    require!(label.len() <= MAX_LABEL_LEN, BettingError::LabelTooLong);
    // Outcomes can only be added to a draft. Once `open_betting` runs, the
    // field of choices is final before the first bet can land.
    require!(
        context.accounts.event.status == EventStatus::Draft,
        BettingError::EventNotDraft
    );

    let index = context.accounts.event.outcome_count;
    *context.accounts.outcome = Outcome {
        event: *context.accounts.event.address(),
        index,
        label,
        total_amount: 0,
        bet_count: 0,
        bump: context.bumps.outcome,
    };

    context.accounts.event.outcome_count += 1;
    Ok(())
}
