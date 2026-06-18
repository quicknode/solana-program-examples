use anchor_lang::prelude::*;

use crate::{error::BettingError, Config, Event, EventStatus};

// Abandon an event that can't be resolved (e.g. the real-world result is void).
// Bettors then reclaim their exact stakes via `claim_refund`; no fee is taken.
#[derive(Accounts)]
pub struct CancelEventAccountConstraints<'info> {
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

pub fn handle_cancel_event(context: Context<CancelEventAccountConstraints>) -> Result<()> {
    require!(
        context.accounts.event.status == EventStatus::Open,
        BettingError::EventNotOpen
    );
    context.accounts.event.status = EventStatus::Cancelled;
    Ok(())
}
