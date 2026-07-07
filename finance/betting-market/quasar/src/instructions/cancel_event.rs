use quasar_lang::prelude::*;

use crate::errors::BettingError;
use crate::state::{snapshot_event, Config, Event, EventStatus};

// Abandon an event that can't be resolved (e.g. the real-world result is void).
// Bettors then reclaim their exact stakes via `claim_refund`; no fee is taken.
#[derive(Accounts)]
pub struct CancelEventAccountConstraints {
    pub admin: Signer,

    #[account(address = Config::seeds(), has_one(admin) @ BettingError::Unauthorized)]
    pub config: Account<Config>,

    #[account(mut, address = Event::seeds(event.event_id.into()))]
    pub event: Account<Event>,
}

#[inline(always)]
pub fn handle_cancel_event(
    accounts: &mut CancelEventAccountConstraints,
) -> Result<(), ProgramError> {
    require!(
        accounts.event.status == EventStatus::Open as u8,
        BettingError::EventNotOpen
    );
    let mut event = snapshot_event(&accounts.event);
    event.status = EventStatus::Cancelled as u8;
    accounts.event.set_inner(event);
    Ok(())
}
