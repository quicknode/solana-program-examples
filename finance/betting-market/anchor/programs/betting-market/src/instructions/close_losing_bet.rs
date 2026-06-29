use anchor_lang::prelude::*;

use crate::{error::BettingError, Bet, Event, EventStatus, User};

// A losing bet pays nothing, but it still occupies a slot in the bettor's
// User index and holds rent. Closing it frees the slot (so the bettor can
// open a new position) and returns the rent. Winning bets must go through
// claim_winnings instead, which also pays out the stake and winnings.
#[derive(Accounts)]
pub struct CloseLosingBetAccountConstraints<'info> {
    #[account(mut)]
    pub bettor: Signer<'info>,

    #[account(
        seeds = [b"event", event.event_id.to_le_bytes().as_ref()],
        bump = event.bump,
    )]
    pub event: Account<'info, Event>,

    #[account(
        mut,
        close = bettor,
        has_one = bettor,
        has_one = event,
        seeds = [b"bet", bet.outcome.as_ref(), bettor.key().as_ref()],
        bump = bet.bump,
    )]
    pub bet: Account<'info, Bet>,

    #[account(
        mut,
        seeds = [b"user", bettor.key().as_ref()],
        bump = user.bump,
    )]
    pub user: Account<'info, User>,
}

pub fn handle_close_losing_bet(context: Context<CloseLosingBetAccountConstraints>) -> Result<()> {
    require!(
        context.accounts.event.status == EventStatus::Settled,
        BettingError::EventNotSettled
    );
    require!(
        context.accounts.bet.outcome_index != context.accounts.event.winning_outcome_index,
        BettingError::BetWon
    );

    let bet_key = context.accounts.bet.key();
    context.accounts.user.remove_bet(&bet_key)?;
    Ok(())
}
