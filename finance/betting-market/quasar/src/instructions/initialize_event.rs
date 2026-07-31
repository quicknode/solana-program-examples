use quasar_lang::prelude::*;
use quasar_spl::prelude::*;

use crate::errors::BettingError;
use crate::state::{
    snapshot_config, Config, Event, EventInner, EventStatus, EventVaultPda, MAX_DESCRIPTION_LEN,
};

#[derive(Accounts)]
#[instruction(event_id: u64)]
pub struct InitializeEventAccountConstraints {
    #[account(mut)]
    pub admin: Signer,

    #[account(
        mut,
        address = Config::seeds(),
        has_one(admin) @ BettingError::Unauthorized,
        has_one(token_mint),
    )]
    pub config: Account<Config>,

    pub token_mint: Account<Mint>,

    #[account(init, payer = admin, address = Event::seeds(event_id))]
    pub event: Account<Event>,

    // The single pool for the whole market: a program-derived token account
    // whose authority is the Event PDA.
    #[account(
        init,
        payer = admin,
        token(mint = token_mint, authority = event, token_program = token_program),
        address = EventVaultPda::seeds(event.address()),
    )]
    pub vault: InterfaceAccount<Token>,

    pub rent: Sysvar<Rent>,
    pub token_program: Program<TokenProgram>,
    pub system_program: Program<SystemProgram>,
}

#[inline(always)]
pub fn handle_initialize_event(
    accounts: &mut InitializeEventAccountConstraints,
    event_id: u64,
    description: &str,
    bumps: &InitializeEventAccountConstraintsBumps,
) -> Result<(), ProgramError> {
    let description_bytes = description.as_bytes();
    require!(
        description_bytes.len() <= MAX_DESCRIPTION_LEN,
        BettingError::DescriptionTooLong
    );

    let mut description_buffer = [0u8; MAX_DESCRIPTION_LEN];
    description_buffer[..description_bytes.len()].copy_from_slice(description_bytes);

    let fee_bps = u16::from(accounts.config.fee_bps);

    accounts.event.set_inner(EventInner {
        event_id,
        outcome_count: 0,
        total_pool: 0,
        status: EventStatus::Open as u8,
        fee_bps,
        winning_outcome_index: 0,
        winning_pool: 0,
        distributable_losing_pool: 0,
        bump: bumps.event,
        description_len: description_bytes.len() as u8,
        description: description_buffer,
    });

    let mut config = snapshot_config(&accounts.config);
    config.event_count = config
        .event_count
        .checked_add(1)
        .ok_or(BettingError::MathOverflow)?;
    accounts.config.set_inner(config);
    Ok(())
}
