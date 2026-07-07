use quasar_lang::prelude::*;
use quasar_spl::prelude::*;

use crate::errors::BettingError;
use crate::state::{
    remove_bet, snapshot_user, Bet, Event, EventStatus, EventVaultPda, User,
};

use super::transfer_from_vault;

#[derive(Accounts)]
pub struct ClaimRefundAccountConstraints {
    #[account(mut)]
    pub bettor: Signer,

    pub token_mint: Account<Mint>,

    #[account(address = Event::seeds(event.event_id.into()))]
    pub event: Account<Event>,

    // Closing the Bet ends the position: the rent goes back to the bettor and a
    // second refund fails because the account no longer exists.
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

    #[account(mut)]
    pub bettor_token_account: Account<Token>,

    #[account(mut, address = EventVaultPda::seeds(event.address()))]
    pub vault: InterfaceAccount<Token>,

    pub token_program: Program<TokenProgram>,
}

#[inline(always)]
pub fn handle_claim_refund(
    accounts: &mut ClaimRefundAccountConstraints,
) -> Result<(), ProgramError> {
    require!(
        accounts.event.status == EventStatus::Cancelled as u8,
        BettingError::EventNotCancelled
    );

    let stake = u64::from(accounts.bet.amount);

    // Drop the Bet from the bettor's index before the transfer (effects before
    // interactions); the Bet account itself closes when the instruction ends.
    let bet_key = *accounts.bet.address();
    let mut user = snapshot_user(&accounts.user);
    remove_bet(&mut user.bets, &mut user.bet_count, &bet_key)?;
    accounts.user.set_inner(user);

    let event_id = u64::from(accounts.event.event_id);
    let event_bump = accounts.event.bump;
    transfer_from_vault(
        &accounts.token_program,
        &accounts.vault,
        &accounts.token_mint,
        &accounts.bettor_token_account,
        &accounts.event,
        stake,
        accounts.token_mint.decimals,
        event_id,
        event_bump,
    )?;

    Ok(())
}
