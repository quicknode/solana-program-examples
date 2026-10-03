use quasar_lang::prelude::*;

use crate::errors::BettingError;
use crate::state::{Bet, Event, EventStatus};

// A losing bet pays nothing, but its account still holds rent. Closing it
// returns the rent to the bettor. Winning bets must go through claim_winnings
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
    )]
    pub bet: Account<Bet>,
}

#[inline(always)]
pub fn handle_close_losing_bet(
    accounts: &mut CloseLosingBetAccountConstraints,
) -> Result<(), ProgramError> {
    // Canonical-PDA check for the bet account. The pre-0.1.0 constraint
    // `address = Bet::seeds(&bet.outcome, ...)` is inexpressible in 0.1.0
    // (an Address-typed stored-data seed cannot both feed client codegen and
    // typecheck onchain), and the generated `Bet::find_address` helper is a
    // const-context/client function whose software SHA-256 exhausts the CU
    // budget onchain. Verifying against the stored bump costs one sha256
    // syscall and rejects non-canonical bet accounts just the same.
    quasar_lang::pda::verify_program_address(
        &Bet::seeds(&accounts.bet.outcome, accounts.bettor.address())
            .with_bump(accounts.bet.bump)
            .as_slices(),
        &crate::ID,
        accounts.bet.address(),
    )?;

    require!(
        accounts.event.status == EventStatus::Settled as u8,
        BettingError::EventNotSettled
    );
    require!(
        accounts.bet.outcome_index != accounts.event.winning_outcome_index,
        BettingError::BetWon
    );

    Ok(())
}
