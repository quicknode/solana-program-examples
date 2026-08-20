use anchor_lang::prelude::*;

use crate::state::Event;

use crate::{error::BettingError, Bet, EventStatus, User};

// A losing bet pays nothing, but it still occupies a slot in the bettor's
// User index and holds rent. Closing it frees the slot (so the bettor can
// open a new position) and returns the rent. Winning bets must go through
// claim_winnings instead, which also pays out the stake and winnings.
#[derive(Accounts)]
pub struct CloseLosingBetAccountConstraints {
    #[account(mut, address = bet.bettor)]
    pub bettor: Signer,

    #[account(
        seeds = [b"event", event.event_id.to_le_bytes()],
        bump = event.bump,
        address = bet.event,
    )]
    pub event: BorshAccount<Event>,

    #[account(
        mut,
        close = bettor,
        seeds = [b"bet", bet.outcome.as_ref(), bettor.address().as_ref()],
        bump = bet.bump,
    )]
    pub bet: BorshAccount<Bet>,

    #[account(
        mut,
        seeds = [b"user", bettor.address().as_ref()],
        bump = user.bump,
    )]
    pub user: BorshAccount<User>,
}

pub fn handle_close_losing_bet(
    context: &mut Context<CloseLosingBetAccountConstraints>,
) -> Result<()> {
    require!(
        context.accounts.event.status == EventStatus::Settled,
        BettingError::EventNotSettled
    );
    require!(
        context.accounts.bet.outcome_index != context.accounts.event.winning_outcome_index,
        BettingError::BetWon
    );

    let bet_key = context.accounts.bet.address();
    context.accounts.user.remove_bet(&bet_key)?;
    Ok(())
}
