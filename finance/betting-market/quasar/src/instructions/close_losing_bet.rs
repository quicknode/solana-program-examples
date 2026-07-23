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
    )]
    pub bet: Account<Bet>,

    #[account(mut, address = User::seeds(bettor.address()))]
    pub user: Account<User>,
}

#[inline(always)]
pub fn handle_close_losing_bet(
    accounts: &mut CloseLosingBetAccountConstraints,
) -> Result<(), ProgramError> {
    // Canonical-PDA check for the bet account. The pre-0.1.0 constraint
    // `address = Bet::seeds(&bet.outcome, ...)` is inexpressible in 0.1.0
    // (an Address-typed stored-data seed cannot both feed client codegen and
    // typecheck on-chain), and the generated `Bet::find_address` helper is a
    // const-context/client function whose software SHA-256 exhausts the CU
    // budget on-chain. Verifying against the stored bump costs one sha256
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

    let bet_key = *accounts.bet.address();
    let mut user = snapshot_user(&accounts.user);
    remove_bet(&mut user.bets, &mut user.bet_count, &bet_key)?;
    accounts.user.set_inner(user);
    Ok(())
}
