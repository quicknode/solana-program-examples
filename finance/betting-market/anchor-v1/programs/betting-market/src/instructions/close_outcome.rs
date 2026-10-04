use anchor_lang::prelude::*;

use crate::{error::BettingError, Config, Event, EventStatus, Outcome};

// Close one Outcome account of a finished event and return its rent to the
// admin, who paid it. Bet accounts derive their address from the outcome's,
// so every bet of the event must already be closed, and the Outcome accounts
// must all close before `close_event` can close the event.
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
    // A Bet account derives its address from its outcome's, so an outcome
    // closed under an open bet would leave that bet's claim or refund with
    // no outcome to check against.
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
