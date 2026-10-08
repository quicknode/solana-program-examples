use anchor_lang::prelude::*;

use crate::{error::BettingError, Config, Event, EventStatus, Outcome};

// Close one Outcome account of a finished event and return its rent to the
// admin, who paid it. The event's accounts close in the reverse of the order
// they were created: every Bet of the event before any Outcome, and every
// Outcome before `close_event` can close the event.
#[derive(Accounts)]
pub struct CloseOutcomeAccountConstraints<'info> {
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
        mut,
        close = admin,
        has_one = event,
        seeds = [b"outcome", event.key().as_ref(), &[outcome.index]],
        bump = outcome.bump,
    )]
    pub outcome: Account<'info, Outcome>,
}

pub fn handle_close_outcome(context: Context<CloseOutcomeAccountConstraints>) -> Result<()> {
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
