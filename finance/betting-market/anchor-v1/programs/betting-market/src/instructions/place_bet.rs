use anchor_lang::prelude::*;
use anchor_spl::{
    associated_token::AssociatedToken,
    token_interface::{Mint, TokenAccount, TokenInterface},
};

use crate::{betting_is_open, error::BettingError, Bet, Config, Event, EventStatus, Outcome};

use super::transfer_tokens_to_vault;

#[derive(Accounts)]
pub struct PlaceBetAccountConstraints<'info> {
    #[account(mut)]
    pub bettor: Signer<'info>,

    #[account(
        seeds = [b"config"],
        bump = config.bump,
        has_one = token_mint,
    )]
    pub config: Account<'info, Config>,

    #[account(mint::token_program = token_program)]
    pub token_mint: Box<InterfaceAccount<'info, Mint>>,

    #[account(
        mut,
        seeds = [b"event", event.event_id.to_le_bytes().as_ref()],
        bump = event.bump,
    )]
    pub event: Box<Account<'info, Event>>,

    #[account(
        mut,
        has_one = event,
        seeds = [b"outcome", event.key().as_ref(), &[outcome.index]],
        bump = outcome.bump,
    )]
    pub outcome: Box<Account<'info, Outcome>>,

    #[account(
        mut,
        associated_token::mint = token_mint,
        associated_token::authority = bettor,
        associated_token::token_program = token_program,
    )]
    pub bettor_token_account: Box<InterfaceAccount<'info, TokenAccount>>,

    #[account(
        mut,
        associated_token::mint = token_mint,
        associated_token::authority = event,
        associated_token::token_program = token_program,
    )]
    pub vault: Box<InterfaceAccount<'info, TokenAccount>>,

    #[account(
        init_if_needed,
        payer = bettor,
        space = Bet::DISCRIMINATOR.len() + Bet::INIT_SPACE,
        seeds = [b"bet", outcome.key().as_ref(), bettor.key().as_ref()],
        bump
    )]
    pub bet: Box<Account<'info, Bet>>,

    pub associated_token_program: Program<'info, AssociatedToken>,
    pub token_program: Interface<'info, TokenInterface>,
    pub system_program: Program<'info, System>,
}

pub fn handle_place_bet(context: Context<PlaceBetAccountConstraints>, amount: u64) -> Result<()> {
    require!(amount > 0, BettingError::ZeroAmount);
    require!(
        context.accounts.event.status == EventStatus::Open,
        BettingError::EventNotOpen
    );
    let now = Clock::get()?.unix_timestamp;
    require!(
        betting_is_open(now, context.accounts.event.betting_closes_at),
        BettingError::BettingClosed
    );

    transfer_tokens_to_vault(
        &context.accounts.bettor_token_account,
        &context.accounts.vault,
        amount,
        &context.accounts.token_mint,
        &context.accounts.bettor,
        &context.accounts.token_program,
    )?;

    let bettor_key = context.accounts.bettor.key();
    let event_key = context.accounts.event.key();
    let outcome_key = context.accounts.outcome.key();
    let outcome_index = context.accounts.outcome.index;
    let bet_bump = context.bumps.bet;

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
    if is_new_bet {
        event.open_bets = event
            .open_bets
            .checked_add(1)
            .ok_or(BettingError::MathOverflow)?;
    }

    Ok(())
}
