use quasar_lang::prelude::*;
use quasar_spl::prelude::*;

use crate::errors::BettingError;
use crate::state::{
    snapshot_event, Config, Event, EventStatus, EventVaultPda, Outcome,
};

use super::transfer_from_vault;

const BPS_DENOMINATOR: u128 = 10_000;

#[derive(Accounts)]
#[instruction(winning_outcome_index: u8)]
pub struct SettleEventAccountConstraints {
    #[account(mut)]
    pub admin: Signer,

    #[account(
        address = Config::seeds(),
        has_one(admin) @ BettingError::Unauthorized,
        has_one(token_mint),
    )]
    pub config: Account<Config>,

    pub token_mint: Account<Mint>,

    #[account(mut, address = Event::seeds(event.event_id.into()))]
    pub event: Account<Event>,

    // The `address` derivation ties this account to `winning_outcome_index`, so
    // a mismatched index can't be settled to.
    #[account(
        has_one(event),
        address = Outcome::seeds(event.address(), winning_outcome_index),
    )]
    pub winning_outcome: Account<Outcome>,

    #[account(mut, address = EventVaultPda::seeds(event.address()))]
    pub vault: InterfaceAccount<Token>,

    // The fee destination. Must be a token account owned by the config's
    // fee_recipient; the transfer verifies the mint.
    #[account(mut)]
    pub fee_recipient_token_account: Account<Token>,

    pub token_program: Program<TokenProgram>,
}

#[inline(always)]
pub fn handle_settle_event(
    accounts: &mut SettleEventAccountConstraints,
    winning_outcome_index: u8,
) -> Result<(), ProgramError> {
    require!(
        accounts.event.status == EventStatus::Open as u8,
        BettingError::EventNotOpen
    );
    let winning_pool = u64::from(accounts.winning_outcome.total_amount);
    require!(winning_pool > 0, BettingError::OutcomeHasNoBets);

    require_keys_eq!(
        accounts.fee_recipient_token_account.owner,
        accounts.config.fee_recipient,
        BettingError::Unauthorized
    );

    let total_pool = u64::from(accounts.event.total_pool);
    let losing_pool = total_pool
        .checked_sub(winning_pool)
        .ok_or(BettingError::MathOverflow)?;

    // The fee is only ever charged on the losing side, so a winner can never
    // receive less than they staked.
    let fee_bps = u16::from(accounts.event.fee_bps);
    let fee: u64 = (losing_pool as u128)
        .checked_mul(fee_bps as u128)
        .ok_or(BettingError::MathOverflow)?
        .checked_div(BPS_DENOMINATOR)
        .ok_or(BettingError::MathOverflow)?
        .try_into()
        .map_err(|_| BettingError::MathOverflow)?;
    let distributable_losing_pool = losing_pool
        .checked_sub(fee)
        .ok_or(BettingError::MathOverflow)?;

    if fee > 0 {
        let event_id = u64::from(accounts.event.event_id);
        let event_bump = accounts.event.bump;
        transfer_from_vault(
            &accounts.token_program,
            &accounts.vault,
            &accounts.token_mint,
            &accounts.fee_recipient_token_account,
            &accounts.event,
            fee,
            accounts.token_mint.decimals,
            event_id,
            event_bump,
        )?;
    }

    let mut event = snapshot_event(&accounts.event);
    event.status = EventStatus::Settled as u8;
    event.winning_outcome_index = winning_outcome_index;
    event.winning_pool = winning_pool;
    event.distributable_losing_pool = distributable_losing_pool;
    accounts.event.set_inner(event);
    Ok(())
}
