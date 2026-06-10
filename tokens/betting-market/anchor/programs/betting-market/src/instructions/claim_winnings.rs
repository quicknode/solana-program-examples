use anchor_lang::prelude::*;
use anchor_spl::token_interface::{Mint, TokenAccount, TokenInterface};

use crate::{error::BettingError, Bet, Event, EventStatus, User};

use super::transfer_tokens_from_vault;

#[derive(Accounts)]
pub struct ClaimWinningsAccountConstraints<'info> {
    #[account(mut)]
    pub bettor: Signer<'info>,

    #[account(mint::token_program = token_program)]
    pub token_mint: InterfaceAccount<'info, Mint>,

    #[account(
        seeds = [b"event", event.event_id.to_le_bytes().as_ref()],
        bump = event.bump,
    )]
    pub event: Account<'info, Event>,

    // Closing the Bet ends the position: the rent goes back to the bettor and
    // a second claim fails because the account no longer exists.
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

    #[account(
        mut,
        associated_token::mint = token_mint,
        associated_token::authority = bettor,
        associated_token::token_program = token_program,
    )]
    pub bettor_token_account: InterfaceAccount<'info, TokenAccount>,

    #[account(
        mut,
        associated_token::mint = token_mint,
        associated_token::authority = event,
        associated_token::token_program = token_program,
    )]
    pub vault: InterfaceAccount<'info, TokenAccount>,

    pub token_program: Interface<'info, TokenInterface>,
}

pub fn handle_claim_winnings(context: Context<ClaimWinningsAccountConstraints>) -> Result<()> {
    require!(
        context.accounts.event.status == EventStatus::Settled,
        BettingError::EventNotSettled
    );
    require!(
        context.accounts.bet.outcome_index == context.accounts.event.winning_outcome_index,
        BettingError::NothingToClaim
    );

    let stake = context.accounts.bet.amount;
    // Total staked on the winning outcome, and the losers' stakes after the fee.
    let winning_pool = context.accounts.event.winning_pool;
    let distributable_losing_pool = context.accounts.event.distributable_losing_pool;

    // Parimutuel split: winners share the losing pool in proportion to their
    // own stake. Work in u128 and divide once, after the multiply, so the
    // result is floored a single time - dividing first would throw away
    // precision. The floor leaves at most a few minor units of dust in the vault.
    let losing_pool_share_numerator = (stake as u128)
        .checked_mul(distributable_losing_pool as u128)
        .ok_or(BettingError::MathOverflow)?;
    let winnings: u64 = losing_pool_share_numerator
        .checked_div(winning_pool as u128)
        .ok_or(BettingError::MathOverflow)?
        .try_into()
        .map_err(|_| BettingError::MathOverflow)?;

    // Winners always get their own stake back on top of their winnings.
    let payout = stake
        .checked_add(winnings)
        .ok_or(BettingError::MathOverflow)?;

    // The position is over, so drop the Bet from the bettor's index before the
    // transfer (effects before interactions); the Bet account itself closes
    // when the instruction finishes.
    let bet_key = context.accounts.bet.key();
    context.accounts.user.remove_bet(&bet_key)?;

    let event_id = context.accounts.event.event_id;
    let event_bump = context.accounts.event.bump;
    transfer_tokens_from_vault(
        &context.accounts.vault,
        &context.accounts.bettor_token_account,
        payout,
        &context.accounts.token_mint,
        &context.accounts.event.to_account_info(),
        &context.accounts.token_program,
        event_id,
        event_bump,
    )?;

    Ok(())
}
