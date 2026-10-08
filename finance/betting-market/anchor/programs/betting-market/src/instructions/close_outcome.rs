use anchor_lang::prelude::*;

use crate::state::Event;

use crate::{error::BettingError, Config, EventStatus, Outcome};

// Close one Outcome account of a finished event and return its rent to the
// admin, who paid it. The event's accounts close in the reverse of the order
// they were created: every Bet of the event before any Outcome, and every
// Outcome before `close_event` can close the event.
#[derive(Accounts)]
pub struct CloseOutcomeAccountConstraints {
    #[account(mut, address = config.admin @ BettingError::Unauthorized)]
    pub admin: Signer,

    #[account(seeds = [b"config"],
        bump = config.bump)]
    pub config: BorshAccount<Config>,

    #[account(
        mut,
        seeds = [b"event", event.event_id.to_le_bytes()],
        bump = event.bump,
        address = outcome.event,
    )]
    pub event: BorshAccount<Event>,

    #[account(
        mut,
        close = admin,
        seeds = [b"outcome", event.address().as_ref(), &[outcome.index]],
        bump = outcome.bump,
    )]
    pub outcome: BorshAccount<Outcome>,
}

pub fn handle_close_outcome(context: &mut Context<CloseOutcomeAccountConstraints>) -> Result<()> {
    require!(
        matches!(
            context.accounts.event.status,
            EventStatus::Settled | EventStatus::Cancelled
        ),
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
        context.accounts.event.open_bets == 0,
        BettingError::BetsStillOpen
    );

    let event = &mut context.accounts.event;
    event.open_outcomes = event
        .open_outcomes
        .checked_sub(1)
        .ok_or(BettingError::MathOverflow)?;
    Ok(())
}
