use anchor_lang::prelude::*;

use crate::{betting_is_open, error::BettingError, Config, Event, EventStatus};

// Move a draft event to Open. From here the outcome list is final and bettors
// can stake, so the question they bet on is fixed before any money arrives.
#[derive(Accounts)]
pub struct OpenBettingAccountConstraints<'info> {
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
}

pub fn handle_open_betting(context: Context<OpenBettingAccountConstraints>) -> Result<()> {
    require!(
        context.accounts.event.status == EventStatus::Draft,
        BettingError::EventNotDraft
    );
    // A market with one outcome has no losing side to pay the winners from.
    require!(
        context.accounts.event.outcome_count >= 2,
        BettingError::NotEnoughOutcomes
    );
    // Opening after the close time would open a market no one can bet on.
    let now = Clock::get()?.unix_timestamp;
    require!(
        betting_is_open(now, context.accounts.event.betting_closes_at),
        BettingError::BettingClosed
    );
    context.accounts.event.status = EventStatus::Open;
    Ok(())
}
