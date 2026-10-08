use anchor_lang::prelude::*;

use crate::state::Event;

use crate::{error::BettingError, Config, EventStatus};

// Abandon an event that can't be resolved (e.g. the real-world result is void).
// Bettors then reclaim their exact stakes via `claim_refund`; no fee is taken.
#[derive(Accounts)]
pub struct CancelEventAccountConstraints {
    #[account(address = config.admin @ BettingError::Unauthorized)]
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
}

pub fn handle_cancel_event(context: &mut Context<CancelEventAccountConstraints>) -> Result<()> {
    // A draft or open event can be cancelled; a settled or cancelled one has
    // already taken its exit.
    require!(
        matches!(
            context.accounts.event.status,
            EventStatus::Draft | EventStatus::Open
        ),
        BettingError::EventNotOpen
    );
    context.accounts.event.status = EventStatus::Cancelled;
    Ok(())
}
