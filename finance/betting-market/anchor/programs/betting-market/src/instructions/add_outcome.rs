use anchor_lang::prelude::*;

use crate::{error::BettingError, Config, Event, EventStatus, Outcome};

pub const MAX_LABEL_LEN: usize = 64;

#[derive(Accounts)]
pub struct AddOutcome<'info> {
    #[account(mut)]
    pub admin: Signer<'info>,

    #[account(
        seeds = [b"config"],
        bump = config.bump,
        has_one = admin @ BettingError::Unauthorized,
    )]
    pub config: Account<'info, Config>,

    #[account(
        mut,
        seeds = [b"event", event.event_id.to_le_bytes().as_ref()],
        bump = event.bump,
    )]
    pub event: Account<'info, Event>,

    #[account(
        init,
        payer = admin,
        space = Outcome::DISCRIMINATOR.len() + Outcome::INIT_SPACE,
        seeds = [b"outcome", event.key().as_ref(), &[event.outcome_count]],
        bump
    )]
    pub outcome: Account<'info, Outcome>,

    pub system_program: Program<'info, System>,
}

pub fn handle_add_outcome(context: Context<AddOutcome>, label: String) -> Result<()> {
    require!(label.len() <= MAX_LABEL_LEN, BettingError::LabelTooLong);
    require!(
        context.accounts.event.status == EventStatus::Open,
        BettingError::EventNotOpen
    );
    // Lock the outcome set once betting starts so the field of choices can't
    // change out from under existing bettors.
    require!(
        context.accounts.event.total_pool == 0,
        BettingError::BettingAlreadyStarted
    );

    let index = context.accounts.event.outcome_count;
    context.accounts.outcome.set_inner(Outcome {
        event: context.accounts.event.key(),
        index,
        label,
        total_amount: 0,
        bet_count: 0,
        bump: context.bumps.outcome,
    });

    context.accounts.event.outcome_count += 1;
    Ok(())
}
