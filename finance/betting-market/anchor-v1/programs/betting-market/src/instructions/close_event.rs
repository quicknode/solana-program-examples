use anchor_lang::prelude::*;
use anchor_spl::{
    associated_token::AssociatedToken,
    token_interface::{Mint, TokenAccount, TokenInterface},
};

use crate::{error::BettingError, Config, Event, EventStatus};

use super::{close_vault, transfer_tokens_from_vault};

// Close a finished event: pay whatever the vault still holds to the fee
// recipient, close the vault, and close the Event account, returning both
// rents to the admin, who paid them. After settlement the vault holds only
// the dust that flooring each winner's share left behind; after a
// cancellation and its refunds it holds nothing.
#[derive(Accounts)]
pub struct CloseEventAccountConstraints<'info> {
    #[account(mut)]
    pub admin: Signer<'info>,

    #[account(
        seeds = [b"config"],
        bump = config.bump,
        has_one = admin @ BettingError::Unauthorized,
        has_one = token_mint,
        has_one = fee_recipient,
    )]
    pub config: Account<'info, Config>,

    #[account(mint::token_program = token_program)]
    pub token_mint: InterfaceAccount<'info, Mint>,

    #[account(
        mut,
        close = admin,
        seeds = [b"event", event.event_id.to_le_bytes().as_ref()],
        bump = event.bump,
    )]
    pub event: Account<'info, Event>,

    #[account(
        mut,
        associated_token::mint = token_mint,
        associated_token::authority = event,
        associated_token::token_program = token_program,
    )]
    pub vault: InterfaceAccount<'info, TokenAccount>,

    /// CHECK: validated against config.fee_recipient by the `has_one` above.
    pub fee_recipient: UncheckedAccount<'info>,

    #[account(
        init_if_needed,
        payer = admin,
        associated_token::mint = token_mint,
        associated_token::authority = fee_recipient,
        associated_token::token_program = token_program,
    )]
    pub fee_recipient_token_account: InterfaceAccount<'info, TokenAccount>,

    pub associated_token_program: Program<'info, AssociatedToken>,
    pub token_program: Interface<'info, TokenInterface>,
    pub system_program: Program<'info, System>,
}

pub fn handle_close_event(context: Context<CloseEventAccountConstraints>) -> Result<()> {
    require!(
        matches!(
            context.accounts.event.status,
            EventStatus::Settled | EventStatus::Cancelled
        ),
        BettingError::EventNotFinished
    );
    // Every claim and refund reads the event, so the event outlives every
    // Bet account of it.
    require!(
        context.accounts.event.open_bets == 0,
        BettingError::BetsStillOpen
    );
    // An Outcome account left behind would be found again, with its old
    // `total_amount`, by a later event created with the same `event_id`.
    require!(
        context.accounts.event.open_outcomes == 0,
        BettingError::OutcomesStillOpen
    );

    let event_id = context.accounts.event.event_id;
    let event_bump = context.accounts.event.bump;

    // Winners' shares are floored, so a settled vault can hold a few minor
    // units nobody has a claim on; they go to the fee recipient with the fee.
    let remainder = context.accounts.vault.amount;
    if remainder > 0 {
        transfer_tokens_from_vault(
            &context.accounts.vault,
            &context.accounts.fee_recipient_token_account,
            remainder,
            &context.accounts.token_mint,
            &context.accounts.event.to_account_info(),
            &context.accounts.token_program,
            event_id,
            event_bump,
        )?;
    }

    // The Event account itself is closed by its `close = admin` constraint.
    close_vault(
        &context.accounts.vault,
        &context.accounts.admin.to_account_info(),
        &context.accounts.event.to_account_info(),
        &context.accounts.token_program,
        event_id,
        event_bump,
    )
}
