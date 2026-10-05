use quasar_lang::prelude::*;

use crate::errors::BettingError;
use crate::state::{snapshot_event, Config, Event, EventStatus, Outcome};

// Close one Outcome account of a finished event and return its rent to the
// admin, who paid it. The event's accounts close in the reverse of the order
// they were created: every Bet of the event before any Outcome, and every
// Outcome before `close_event` can close the event.
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
    // No claim, refund or losing-bet close reads the Outcome: each re-derives
    // the Bet address from the outcome pubkey the Bet stores, which stays
    // valid as bytes after the Outcome closes. This check is an ordering
    // rule, not something those handlers need: an Outcome stays open for as
    // long as any Bet of the event names it by address, so no live position
    // ever points at an address the program has emptied, and the event's
    // `open_bets` counter is the gate.
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
