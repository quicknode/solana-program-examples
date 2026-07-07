use quasar_lang::prelude::*;

use crate::errors::BettingError;
use crate::state::{remove_bet, snapshot_user, Bet, Event, EventStatus, User};

// A losing bet pays nothing, but it still occupies a slot in the bettor's User
// index and holds rent. Closing it frees the slot (so the bettor can open a new
// position) and returns the rent. Winning bets must go through claim_winnings
// instead, which also pays out the stake and winnings.
#[derive(Accounts)]
pub struct CloseLosingBetAccountConstraints {
    #[account(mut)]
    pub bettor: Signer,

    #[account(address = Event::seeds(event.event_id.into()))]
    pub event: Account<Event>,

    #[account(
        mut,
        close(dest = bettor),
        has_one(bettor),
        has_one(event),
        address = Bet::seeds(&bet.outcome, bettor.address()),
    )]
    pub bet: Account<Bet>,

    #[account(mut, address = User::seeds(bettor.address()))]
    pub user: Account<User>,
}

#[inline(always)]
pub fn handle_close_losing_bet(
    accounts: &mut CloseLosingBetAccountConstraints,
) -> Result<(), ProgramError> {
    require!(
        accounts.event.status == EventStatus::Settled as u8,
        BettingError::EventNotSettled
    );
    require!(
        accounts.bet.outcome_index != accounts.event.winning_outcome_index,
        BettingError::BetWon
    );

    let bet_key = *accounts.bet.address();
    let mut user = snapshot_user(&accounts.user);
    remove_bet(&mut user.bets, &mut user.bet_count, &bet_key)?;
    accounts.user.set_inner(user);
    Ok(())
}
