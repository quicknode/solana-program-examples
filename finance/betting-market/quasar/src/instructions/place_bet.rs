use quasar_lang::prelude::*;
use quasar_spl::prelude::*;

use crate::errors::BettingError;
use crate::state::{
    add_bet, snapshot_event, snapshot_outcome, snapshot_user, Bet, BetInner, Config, Event,
    EventStatus, EventVaultPda, Outcome, User,
};

use super::transfer_to_vault;

#[derive(Accounts)]
pub struct PlaceBetAccountConstraints {
    #[account(mut)]
    pub bettor: Signer,

    #[account(address = Config::seeds(), has_one(token_mint))]
    pub config: Account<Config>,

    pub token_mint: Account<Mint>,

    #[account(mut, address = Event::seeds(event.event_id.into()))]
    pub event: Account<Event>,

    #[account(
        mut,
        has_one(event),
        address = Outcome::seeds(event.address(), outcome.index),
    )]
    pub outcome: Account<Outcome>,

    #[account(mut)]
    pub bettor_token_account: Account<Token>,

    #[account(mut, address = EventVaultPda::seeds(event.address()))]
    pub vault: InterfaceAccount<Token>,

    // init(idempotent): the first bet on this outcome creates the Bet; a
    // re-bet reuses it and tops up `amount`.
    #[account(
        init(idempotent),
        payer = bettor,
        address = Bet::seeds(outcome.address(), bettor.address()),
    )]
    pub bet: Account<Bet>,

    #[account(
        init(idempotent),
        payer = bettor,
        address = User::seeds(bettor.address()),
    )]
    pub user: Account<User>,

    pub rent: Sysvar<Rent>,
    pub token_program: Program<TokenProgram>,
    pub system_program: Program<SystemProgram>,
}

#[inline(always)]
pub fn handle_place_bet(
    accounts: &mut PlaceBetAccountConstraints,
    amount: u64,
    bumps: &PlaceBetAccountConstraintsBumps,
) -> Result<(), ProgramError> {
    require!(amount > 0, BettingError::ZeroAmount);
    require!(
        accounts.event.status == EventStatus::Open as u8,
        BettingError::EventNotOpen
    );

    transfer_to_vault(
        &accounts.token_program,
        &accounts.bettor_token_account,
        &accounts.token_mint,
        &accounts.vault,
        &accounts.bettor,
        amount,
        accounts.token_mint.decimals,
    )?;

    let bettor_key = *accounts.bettor.address();
    let event_key = *accounts.event.address();
    let outcome_key = *accounts.outcome.address();
    let outcome_index = accounts.outcome.index;
    let bet_key = *accounts.bet.address();

    // A fresh init(idempotent) Bet has amount 0; that is how we tell a first
    // bet on this outcome from a top-up, and it gates the per-outcome and
    // per-user bookkeeping.
    let current_amount = u64::from(accounts.bet.amount);
    let is_new_bet = current_amount == 0;
    let new_amount = current_amount
        .checked_add(amount)
        .ok_or(BettingError::MathOverflow)?;

    accounts.bet.set_inner(BetInner {
        bettor: bettor_key,
        event: event_key,
        outcome: outcome_key,
        outcome_index,
        amount: new_amount,
        bump: bumps.bet,
    });

    let mut outcome = snapshot_outcome(&accounts.outcome);
    outcome.total_amount = outcome
        .total_amount
        .checked_add(amount)
        .ok_or(BettingError::MathOverflow)?;
    if is_new_bet {
        outcome.bet_count = outcome
            .bet_count
            .checked_add(1)
            .ok_or(BettingError::MathOverflow)?;
    }
    accounts.outcome.set_inner(outcome);

    let mut event = snapshot_event(&accounts.event);
    event.total_pool = event
        .total_pool
        .checked_add(amount)
        .ok_or(BettingError::MathOverflow)?;
    accounts.event.set_inner(event);

    let mut user = snapshot_user(&accounts.user);
    if user.authority == Address::default() {
        user.authority = bettor_key;
        user.bump = bumps.user;
    }
    if is_new_bet {
        add_bet(&mut user.bets, &mut user.bet_count, &bet_key)?;
    }
    accounts.user.set_inner(user);

    Ok(())
}
