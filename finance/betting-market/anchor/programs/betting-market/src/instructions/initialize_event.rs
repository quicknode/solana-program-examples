use anchor_lang::prelude::*;

use crate::state::Event;
use anchor_spl::mint;
use anchor_spl::{
    associated_token::AssociatedToken,
    token_interface::{Mint, TokenAccount, TokenInterface},
};

use crate::{error::BettingError, Config, EventStatus};

pub const MAX_DESCRIPTION_LEN: usize = 200;

#[derive(Accounts)]
#[instruction(event_id: u64)]
pub struct InitializeEventAccountConstraints {
    #[account(mut, address = config.admin @ BettingError::Unauthorized)]
    pub admin: Signer,

    #[account(mut,
        seeds = [b"config"],
        bump = config.bump)]
    pub config: BorshAccount<Config>,

    #[account(mint::token_program = token_program, address = config.token_mint)]
    pub token_mint: InterfaceAccount<Mint>,

    #[account(
        init,
        payer = admin,
        space = Event::DISCRIMINATOR.len() + Event::INIT_SPACE,
        seeds = [b"event", event_id.to_le_bytes()],
        bump
    )]
    pub event: BorshAccount<Event>,

    // The single pool for the whole market: an ATA owned by the Event PDA.
    #[account(
        init,
        payer = admin,
        associated_token::mint = token_mint,
        associated_token::authority = event,
        associated_token::token_program = token_program
    )]
    pub vault: InterfaceAccount<TokenAccount>,

    pub associated_token_program: Program<AssociatedToken>,
    pub token_program: Interface<'static, TokenInterface>,
    pub system_program: Program<System>,
}

pub fn handle_initialize_event(
    context: &mut Context<InitializeEventAccountConstraints>,
    event_id: u64,
    description: String,
) -> Result<()> {
    require!(
        description.len() <= MAX_DESCRIPTION_LEN,
        BettingError::DescriptionTooLong
    );

    *context.accounts.event = Event {
        event_id,
        description,
        outcome_count: 0,
        total_pool: 0,
        status: EventStatus::Open,
        fee_bps: context.accounts.config.fee_bps,
        winning_outcome_index: 0,
        winning_pool: 0,
        distributable_losing_pool: 0,
        bump: context.bumps.event,
    };

    context.accounts.config.event_count += 1;
    Ok(())
}
