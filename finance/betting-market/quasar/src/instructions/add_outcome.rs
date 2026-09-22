use quasar_lang::prelude::*;

use crate::errors::BettingError;
use crate::state::{
    snapshot_event, Config, Event, EventStatus, Outcome, OutcomeInner, MAX_LABEL_LEN,
};

#[derive(Accounts)]
pub struct AddOutcomeAccountConstraints {
    #[account(mut)]
    pub admin: Signer,

    #[account(address = Config::seeds(), has_one(admin) @ BettingError::Unauthorized)]
    pub config: Account<Config>,

    #[account(mut, address = Event::seeds(event.event_id.into()))]
    pub event: Account<Event>,

    #[account(
        init,
        payer = admin,
        address = Outcome::seeds(event.address(), event.outcome_count),
    )]
    pub outcome: Account<Outcome>,

    pub rent: Sysvar<Rent>,
    pub system_program: Program<SystemProgram>,
}

#[inline(always)]
pub fn handle_add_outcome(
    accounts: &mut AddOutcomeAccountConstraints,
    label: &str,
    bumps: &AddOutcomeAccountConstraintsBumps,
) -> Result<(), ProgramError> {
    let label_bytes = label.as_bytes();
    require!(
        label_bytes.len() <= MAX_LABEL_LEN,
        BettingError::LabelTooLong
    );
    // Outcomes can only be added to a draft. Once `open_betting` runs, the
    // field of choices is final before the first bet can land.
    require!(
        accounts.event.status == EventStatus::Draft as u8,
        BettingError::EventNotDraft
    );

    let index = accounts.event.outcome_count;
    let mut label_buffer = [0u8; MAX_LABEL_LEN];
    label_buffer[..label_bytes.len()].copy_from_slice(label_bytes);

    accounts.outcome.set_inner(OutcomeInner {
        event: *accounts.event.address(),
        index,
        total_amount: 0,
        bet_count: 0,
        bump: bumps.outcome,
        label_len: label_bytes.len() as u8,
        label: label_buffer,
    });

    let mut event = snapshot_event(&accounts.event);
    event.outcome_count = event
        .outcome_count
        .checked_add(1)
        .ok_or(BettingError::MathOverflow)?;
    accounts.event.set_inner(event);
    Ok(())
}
