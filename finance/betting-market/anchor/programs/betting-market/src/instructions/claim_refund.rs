use anchor_lang::prelude::*;

use crate::state::Event;
use anchor_spl::mint;
use anchor_spl::token_interface::{Mint, TokenAccount, TokenInterface};

use crate::{error::BettingError, Bet, EventStatus, User};

use super::{transfer_tokens_from_vault, EventSigner};

#[derive(Accounts)]
pub struct ClaimRefundAccountConstraints {
    #[account(mut, address = bet.bettor)]
    pub bettor: Signer,

    #[account(mint::token_program = token_program)]
    pub token_mint: InterfaceAccount<Mint>,

    // `mut` so the borrow released for the vault CPI below can be reacquired:
    // v2 has no read-only reacquire, and the derive dereferences `event` again
    // when it checks the constraints that name it.
    #[account(
        mut,
        seeds = [b"event", event.event_id.to_le_bytes()],
        bump = event.bump,
        address = bet.event,
    )]
    pub event: BorshAccount<Event>,

    // Closing the Bet ends the position: the rent goes back to the bettor and
    // a second refund fails because the account no longer exists.
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
    context.accounts.user.remove_bet(bet_key)?;

    // Gather the signing material before the borrow goes away.
    let event_signer = EventSigner::new(&context.accounts.event);
    // `event` signs the transfer below. Release its borrow across
    // the CPI: the runtime rejects a CPI that borrows an account we hold.
    context.accounts.event.release_borrow()?;

    transfer_tokens_from_vault(
        &mut context.accounts.vault,
        &mut context.accounts.bettor_token_account,
        stake,
        &context.accounts.token_mint,
        &event_signer,
        &context.accounts.token_program,
    )?;

    // Take the borrow back before the derive's exit path touches `event` again.
    context.accounts.event.reacquire_borrow_mut()?;

    Ok(())
}
