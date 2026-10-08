use anchor_lang::prelude::*;

use crate::{error::BettingError, Bet, Event, EventStatus};

// A losing bet pays nothing, but its account still holds rent. Closing it
// returns the rent to the bettor. Winning bets must go through claim_winnings
// instead, which also pays out the stake and winnings.
#[derive(Accounts)]
pub struct CloseLosingBetAccountConstraints<'info> {
    #[account(mut)]
    pub bettor: Signer<'info>,

    #[account(
        mut,
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

    // This Bet account closes when the handler returns, so the event's count
    // of open bets drops by one.
    context.accounts.event.open_bets = context
        .accounts
        .event
        .open_bets
        .checked_sub(1)
        .ok_or(BettingError::MathOverflow)?;

    Ok(())
}
