use anchor_lang::prelude::*;
use anchor_spl::mint;
use anchor_spl::token_interface::{Mint, TokenAccount, TokenInterface};

use crate::{error::BettingError, Bet, Event, EventStatus, User};

use super::transfer_tokens_from_vault;

#[derive(Accounts)]
pub struct ClaimRefundAccountConstraints {
    #[account(mut)]
    pub bettor: Signer,

    #[account(mint::token_program = token_program)]
    pub token_mint: InterfaceAccount<Mint>,

    #[account(
        seeds = [b"event", event.event_id.to_le_bytes().as_ref()],
        bump = event.bump,
    )]
    pub event: BorshAccount<Event>,

    // Closing the Bet ends the position: the rent goes back to the bettor and
    // a second refund fails because the account no longer exists.
    #[account(
        mut,
        close = bettor,
        has_one = bettor,
        has_one = event,
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

    #[account(
        mut,
        associated_token::mint = token_mint,
        associated_token::authority = bettor,
        associated_token::token_program = token_program,
    )]
    pub bettor_token_account: InterfaceAccount<TokenAccount>,

    #[account(
        mut,
        associated_token::mint = token_mint,
        associated_token::authority = event,
        associated_token::token_program = token_program,
    )]
    pub vault: InterfaceAccount<TokenAccount>,

    pub token_program: Interface<'static, TokenInterface>,
}

pub fn handle_claim_refund(context: &mut Context<ClaimRefundAccountConstraints>) -> Result<()> {
    require!(
        context.accounts.event.status == EventStatus::Cancelled,
        BettingError::EventNotCancelled
    );

    let stake = context.accounts.bet.amount;

    // The position is over, so drop the Bet from the bettor's index before the
    // transfer (effects before interactions); the Bet account itself closes
    // when the instruction finishes.
    let bet_key = context.accounts.bet.address();
    context.accounts.user.remove_bet(&bet_key)?;

    let event_id = context.accounts.event.event_id;
    let event_bump = context.accounts.event.bump;
    transfer_tokens_from_vault(
        &context.accounts.vault,
        &context.accounts.bettor_token_account,
        stake,
        &context.accounts.token_mint,
        &context.accounts.event.cpi_handle_mut(),
        &context.accounts.token_program,
        event_id,
        event_bump,
    )?;

    Ok(())
}
