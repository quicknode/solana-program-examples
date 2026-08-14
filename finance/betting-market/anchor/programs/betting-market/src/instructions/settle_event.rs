use anchor_lang::prelude::*;

use crate::state::Event;
use anchor_spl::mint;
use anchor_spl::{
    associated_token::AssociatedToken,
    token_interface::{Mint, TokenAccount, TokenInterface},
};

use crate::{error::BettingError, Config, EventStatus, Outcome};

use super::transfer_tokens_from_vault;

const BPS_DENOMINATOR: u128 = 10_000;

#[derive(Accounts)]
#[instruction(winning_outcome_index: u8)]
pub struct SettleEventAccountConstraints {
    #[account(mut)]
    pub admin: Signer,

    #[account(
        seeds = [b"config"],
        bump = config.bump,
        has_one = admin @ BettingError::Unauthorized,
        has_one = token_mint,
        has_one = fee_recipient,
    )]
    pub config: BorshAccount<Config>,

    #[account(mint::token_program = token_program)]
    pub token_mint: InterfaceAccount<Mint>,

    #[account(
        mut,
        seeds = [b"event", event.event_id.to_le_bytes()],
        bump = event.bump,
    )]
    pub event: BorshAccount<Event>,

    #[account(
        has_one = event,
        seeds = [b"outcome", event.address().as_ref(), &[winning_outcome_index]],
        bump = winning_outcome.bump,
    )]
    pub winning_outcome: BorshAccount<Outcome>,

    #[account(
        mut,
        associated_token::mint = token_mint,
        associated_token::authority = event,
        associated_token::token_program = token_program,
    )]
    pub vault: InterfaceAccount<TokenAccount>,

    /// CHECK: validated against config.fee_recipient by the `has_one` above.
    pub fee_recipient: UncheckedAccount,

    #[account(
        init_if_needed,
        payer = admin,
        associated_token::mint = token_mint,
        associated_token::authority = fee_recipient,
        associated_token::token_program = token_program,
    )]
    pub fee_recipient_token_account: InterfaceAccount<TokenAccount>,

    pub associated_token_program: Program<AssociatedToken>,
    pub token_program: Interface<'static, TokenInterface>,
    pub system_program: Program<System>,
}

pub fn handle_settle_event(
    context: &mut Context<SettleEventAccountConstraints>,
    winning_outcome_index: u8,
) -> Result<()> {
    require!(
        context.accounts.event.status == EventStatus::Open,
        BettingError::EventNotOpen
    );
    require!(
        context.accounts.winning_outcome.total_amount > 0,
        BettingError::OutcomeHasNoBets
    );

    let winning_pool = context.accounts.winning_outcome.total_amount;
    let total_pool = context.accounts.event.total_pool;
    let losing_pool = total_pool - winning_pool;

    // Winners always get their own stake back; the fee is only ever charged on
    // the losing side, so a winner can never receive less than they staked.
    let fee =
        (losing_pool as u128 * context.accounts.event.fee_bps as u128 / BPS_DENOMINATOR) as u64;
    let distributable_losing_pool = losing_pool - fee;

    if fee > 0 {
        let event_id = context.accounts.event.event_id;
        let event_bump = context.accounts.event.bump;
        transfer_tokens_from_vault(
            &mut context.accounts.vault,
            &mut context.accounts.fee_recipient_token_account,
            fee,
            &context.accounts.token_mint,
            *context.accounts.event.account(),
            &context.accounts.token_program,
            event_id,
            event_bump,
        )?;
    }

    let event = &mut context.accounts.event;
    event.status = EventStatus::Settled;
    event.winning_outcome_index = winning_outcome_index;
    event.winning_pool = winning_pool;
    event.distributable_losing_pool = distributable_losing_pool;
    Ok(())
}
