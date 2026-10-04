use quasar_lang::prelude::*;
use quasar_spl::prelude::*;

use crate::errors::BettingError;
use crate::state::{Config, Event, EventStatus, EventVaultPda};

use super::{close_vault, transfer_from_vault};

// Close a finished event: pay whatever the vault still holds to the fee
// recipient, close the vault, and close the Event account, returning both
// rents to the admin, who paid them. After settlement the vault holds only
// the dust that flooring each winner's share left behind; after a
// cancellation and its refunds it holds nothing.
#[derive(Accounts)]
pub struct CloseEventAccountConstraints {
    #[account(mut)]
    pub admin: Signer,

    #[account(
        address = Config::seeds(),
        has_one(admin) @ BettingError::Unauthorized,
        has_one(token_mint),
    )]
    pub config: Account<Config>,

    pub token_mint: Account<Mint>,

    // The `close(dest = admin)` constraint closes the Event account once the
    // handler has closed the vault it signs for.
    #[account(
        mut,
        close(dest = admin),
        address = Event::seeds(event.event_id.into()),
    )]
    pub event: Account<Event>,

    #[account(mut, address = EventVaultPda::seeds(event.address()))]
    pub vault: InterfaceAccount<Token>,

    // Where the vault's remainder goes. Must be a token account owned by the
    // config's fee_recipient, as in `settle_event`; the transfer verifies the
    // mint.
    #[account(mut)]
    pub fee_recipient_token_account: Account<Token>,

    pub token_program: Program<TokenProgram>,
}

#[inline(always)]
pub fn handle_close_event(accounts: &mut CloseEventAccountConstraints) -> Result<(), ProgramError> {
    require!(
        accounts.event.status == EventStatus::Settled as u8
            || accounts.event.status == EventStatus::Cancelled as u8,
        BettingError::EventNotFinished
    );
    // Every claim and refund reads the event, so the event outlives every
    // Bet account of it.
    require!(
        u64::from(accounts.event.open_bets) == 0,
        BettingError::BetsStillOpen
    );
    // An Outcome account left behind would be found again, with its old
    // `total_amount`, by a later event created with the same `event_id`.
    require!(
        accounts.event.open_outcomes == 0,
        BettingError::OutcomesStillOpen
    );

    require_keys_eq!(
        accounts.fee_recipient_token_account.owner,
        accounts.config.fee_recipient,
        BettingError::Unauthorized
    );

    let event_id = u64::from(accounts.event.event_id);
    let event_bump = accounts.event.bump;

    // Winners' shares are floored, so a settled vault can hold a few minor
    // units nobody has a claim on; they go to the fee recipient with the fee.
    let remainder = accounts.vault.amount();
    if remainder > 0 {
        transfer_from_vault(
            &accounts.token_program,
            &accounts.vault,
            &accounts.token_mint,
            &accounts.fee_recipient_token_account,
            &accounts.event,
            remainder,
            accounts.token_mint.decimals,
            event_id,
            event_bump,
        )?;
    }

    // The Event account itself is closed by its `close(dest = admin)`
    // constraint.
    close_vault(
        &accounts.token_program,
        &accounts.vault,
        &accounts.admin,
        &accounts.event,
        event_id,
        event_bump,
    )
}
