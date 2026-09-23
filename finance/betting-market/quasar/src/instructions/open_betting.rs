use quasar_lang::{prelude::*, sysvars::Sysvar as _};

use crate::errors::BettingError;
use crate::state::{betting_is_open, snapshot_event, Config, Event, EventStatus};

// Move a draft event to Open. From here the outcome list is final and bettors
// can stake, so the question they bet on is fixed before any money arrives.
#[derive(Accounts)]
pub struct OpenBettingAccountConstraints {
    pub admin: Signer,

    #[account(address = Config::seeds(), has_one(admin) @ BettingError::Unauthorized)]
    pub config: Account<Config>,

    #[account(mut, address = Event::seeds(event.event_id.into()))]
    pub event: Account<Event>,
}

#[inline(always)]
pub fn handle_open_betting(
    accounts: &mut OpenBettingAccountConstraints,
) -> Result<(), ProgramError> {
    require!(
        accounts.event.status == EventStatus::Draft as u8,
        BettingError::EventNotDraft
    );
    // A market with one outcome has no losing side to pay the winners from.
    require!(
        accounts.event.outcome_count >= 2,
        BettingError::NotEnoughOutcomes
    );
    // Opening after the close time would open a market no one can bet on.
    let now: i64 = Clock::get()?.unix_timestamp.into();
    require!(
        betting_is_open(now, i64::from(accounts.event.betting_closes_at)),
        BettingError::BettingClosed
    );
    let mut event = snapshot_event(&accounts.event);
    event.status = EventStatus::Open as u8;
    accounts.event.set_inner(event);
    Ok(())
}
