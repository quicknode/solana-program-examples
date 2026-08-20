use anchor_lang::prelude::*;

use crate::state::Event;
use anchor_spl::mint;
use anchor_spl::{
    associated_token::AssociatedToken,
    token_interface::{Mint, TokenAccount, TokenInterface},
};

use crate::{error::BettingError, Bet, Config, EventStatus, Outcome, User, MAX_BETS_PER_USER};

use super::transfer_tokens_to_vault;

#[derive(Accounts)]
pub struct PlaceBetAccountConstraints {
    #[account(mut)]
    pub bettor: Signer,

    #[account(seeds = [b"config"],
        bump = config.bump)]
    pub config: BorshAccount<Config>,

    #[account(mint::token_program = token_program, address = config.token_mint)]
    pub token_mint: Box<InterfaceAccount<Mint>>,

    #[account(
        mut,
        seeds = [b"event", event.event_id.to_le_bytes()],
        bump = event.bump,
        address = outcome.event,
    )]
    pub event: Box<BorshAccount<Event>>,

    #[account(
        mut,
        seeds = [b"outcome", event.address().as_ref(), &[outcome.index]],
        bump = outcome.bump,
    )]
    pub outcome: Box<BorshAccount<Outcome>>,

    #[account(
        mut,
        associated_token::mint = token_mint,
        associated_token::authority = bettor,
        associated_token::token_program = token_program,
    )]
    pub bettor_token_account: Box<InterfaceAccount<TokenAccount>>,

    #[account(
        mut,
        associated_token::mint = token_mint,
        associated_token::authority = event,
        associated_token::token_program = token_program,
    )]
    pub vault: Box<InterfaceAccount<TokenAccount>>,

    #[account(
        init_if_needed,
        payer = bettor,
        space = Bet::DISCRIMINATOR.len() + Bet::INIT_SPACE,
        seeds = [b"bet", outcome.address().as_ref(), bettor.address().as_ref()],
        bump
    )]
    pub bet: Box<BorshAccount<Bet>>,

    #[account(
        init_if_needed,
        payer = bettor,
        space = User::DISCRIMINATOR.len() + User::INIT_SPACE,
        seeds = [b"user", bettor.address().as_ref()],
        bump
    )]
    pub user: Box<BorshAccount<User>>,

    pub associated_token_program: Program<AssociatedToken>,
    pub token_program: Interface<'static, TokenInterface>,
    pub system_program: Program<System>,
}

pub fn handle_place_bet(
    context: &mut Context<PlaceBetAccountConstraints>,
    amount: u64,
) -> Result<()> {
    require!(amount > 0, BettingError::ZeroAmount);
    require!(
        context.accounts.event.status == EventStatus::Open,
        BettingError::EventNotOpen
    );

    transfer_tokens_to_vault(
        &mut context.accounts.bettor_token_account,
        &mut context.accounts.vault,
        amount,
        &context.accounts.token_mint,
        &context.accounts.bettor,
        &context.accounts.token_program,
    )?;

    let bettor_key = *context.accounts.bettor.address();
    let event_key = *context.accounts.event.address();
    let outcome_key = *context.accounts.outcome.address();
    let outcome_index = context.accounts.outcome.index;
    let bet_key = *context.accounts.bet.address();
    let bet_bump = context.bumps.bet;
    let user_bump = context.bumps.user;

    let bet = &mut context.accounts.bet;
    // A fresh init_if_needed Bet has amount 0; that is how we tell a first bet
    // on this outcome from a top-up, and it gates the per-outcome bookkeeping.
    let is_new_bet = bet.amount == 0;
    if is_new_bet {
        bet.bettor = bettor_key;
        bet.event = event_key;
        bet.outcome = outcome_key;
        bet.outcome_index = outcome_index;
        bet.bump = bet_bump;
    }
    bet.amount = bet
        .amount
        .checked_add(amount)
        .ok_or(BettingError::MathOverflow)?;

    let outcome = &mut context.accounts.outcome;
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

    let event = &mut context.accounts.event;
    event.total_pool = event
        .total_pool
        .checked_add(amount)
        .ok_or(BettingError::MathOverflow)?;

    let user = &mut context.accounts.user;
    if user.authority == Address::default() {
        user.authority = bettor_key;
        user.bump = user_bump;
    }
    if is_new_bet {
        require!(
            user.bets.len() < MAX_BETS_PER_USER,
            BettingError::TooManyBets
        );
        user.bets.push(bet_key);
    }

    Ok(())
}
