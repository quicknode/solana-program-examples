use quasar_lang::prelude::*;

use crate::errors::BettingError;
use crate::state::{snapshot_event, Config, Event, EventStatus, Outcome};

// Close one Outcome account of a finished event and return its rent to the
// admin, who paid it. Bet accounts derive their address from the outcome's,
// so every bet of the event must already be closed, and the Outcome accounts
// must all close before `close_event` can close the event.
#[derive(Accounts)]
pub struct CloseOutcomeAccountConstraints {
    #[account(mut)]
    pub admin: Signer,

    #[account(address = Config::seeds(), has_one(admin) @ BettingError::Unauthorized)]
    pub config: Account<Config>,

    #[account(mut, address = Event::seeds(event.event_id.into()))]
    pub event: Account<Event>,

    #[account(
        mut,
        close(dest = admin),
        has_one(event),
        address = Outcome::seeds(event.address(), outcome.index),
    )]
    pub outcome: Account<Outcome>,
}

#[inline(always)]
pub fn handle_close_outcome(
    accounts: &mut CloseOutcomeAccountConstraints,
) -> Result<(), ProgramError> {
    require!(
        accounts.event.status == EventStatus::Settled as u8
            || accounts.event.status == EventStatus::Cancelled as u8,
        BettingError::EventNotFinished
    );
    // A Bet account derives its address from its outcome's, so an outcome
    // closed under an open bet would leave that bet's claim or refund with
    // no outcome to check against.
    require!(
        u64::from(accounts.event.open_bets) == 0,
        BettingError::BetsStillOpen
    );

    let mut event = snapshot_event(&accounts.event);
    event.open_outcomes = event
        .open_outcomes
        .checked_sub(1)
        .ok_or(BettingError::MathOverflow)?;
    accounts.event.set_inner(event);
    Ok(())
}
