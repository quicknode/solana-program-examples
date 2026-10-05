use anchor_lang::prelude::*;

use crate::state::Event;
use anchor_spl::mint;
use anchor_spl::{
    associated_token::AssociatedToken,
    token_interface::{Mint, TokenAccount, TokenInterface},
};

use crate::{error::BettingError, Config, EventStatus};

use super::{close_vault, transfer_tokens_from_vault, EventSigner};

// Close a finished event: pay whatever the vault still holds to the fee
// recipient, close the vault, and close the Event account, returning both
// rents to the admin, who paid them. After settlement the vault holds only
// the dust that flooring each winner's share left behind; after a
// cancellation and its refunds it holds nothing.
#[derive(Accounts)]
pub struct CloseEventAccountConstraints {
    #[account(mut, address = config.admin @ BettingError::Unauthorized)]
    pub admin: Signer,

    #[account(seeds = [b"config"],
        bump = config.bump)]
    pub config: BorshAccount<Config>,

    #[account(mint::token_program = token_program, address = config.token_mint)]
    pub token_mint: InterfaceAccount<Mint>,

    #[account(
        mut,
        close = admin,
        seeds = [b"event", event.event_id.to_le_bytes()],
        bump = event.bump,
    )]
    pub event: BorshAccount<Event>,

    #[account(
        mut,
        associated_token::mint = token_mint,
        associated_token::authority = event,
        associated_token::token_program = token_program,
    )]
    pub vault: InterfaceAccount<TokenAccount>,

    /// CHECK: validated against config.fee_recipient by the `address` constraint.
    #[account(address = config.fee_recipient)]
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

pub fn handle_close_event(context: &mut Context<CloseEventAccountConstraints>) -> Result<()> {
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
    // Every Outcome account of the event must already be closed. Outcome
    // addresses derive from the event's, and the event's from the
    // admin-supplied `event_id`, so a later event created with the same
    // `event_id` reuses this event's address and its outcome addresses. An
    // Outcome left behind would make that event's first `add_outcome` fail,
    // because `init` refuses an account that already exists.
    require!(
        context.accounts.event.open_outcomes == 0,
        BettingError::OutcomesStillOpen
    );

    // Gather the signing material before the borrow goes away.
    let event_signer = EventSigner::new(&context.accounts.event);
    // `event` signs both CPIs below. Release its borrow across them: the
    // runtime rejects a CPI that borrows an account we hold.
    context.accounts.event.release_borrow()?;

    // Winners' shares are floored, so a settled vault can hold a few minor
    // units nobody has a claim on; they go to the fee recipient with the fee.
    let remainder = context.accounts.vault.amount();
    if remainder > 0 {
        transfer_tokens_from_vault(
            &mut context.accounts.vault,
            &mut context.accounts.fee_recipient_token_account,
            remainder,
            &context.accounts.token_mint,
            &event_signer,
            &context.accounts.token_program,
        )?;
    }

    close_vault(
        &mut context.accounts.vault,
        &mut context.accounts.admin,
        &event_signer,
        &context.accounts.token_program,
    )?;

    // Take the borrow back before the derive's exit path closes `event`
    // through its `close = admin` constraint.
    context.accounts.event.reacquire_borrow_mut()?;

    Ok(())
}
