use anchor_lang::prelude::*;
use anchor_spl::{
    associated_token::AssociatedToken,
    token_interface::{Mint, TokenAccount, TokenInterface},
};

use crate::{error::BettingError, Config, Event, EventStatus};

pub const MAX_DESCRIPTION_LEN: usize = 200;

#[derive(Accounts)]
#[instruction(event_id: u64)]
pub struct CreateEvent<'info> {
    #[account(mut)]
    pub admin: Signer<'info>,

    #[account(
        mut,
        seeds = [b"config"],
        bump = config.bump,
        has_one = admin @ BettingError::Unauthorized,
        has_one = token_mint,
    )]
    pub config: Account<'info, Config>,

    #[account(mint::token_program = token_program)]
    pub token_mint: InterfaceAccount<'info, Mint>,

    #[account(
        init,
        payer = admin,
        space = Event::DISCRIMINATOR.len() + Event::INIT_SPACE,
        seeds = [b"event", event_id.to_le_bytes().as_ref()],
        bump
    )]
    pub event: Account<'info, Event>,

    // The single pool for the whole market: an ATA owned by the Event PDA.
    #[account(
        init,
        payer = admin,
        associated_token::mint = token_mint,
        associated_token::authority = event,
        associated_token::token_program = token_program
    )]
    pub vault: InterfaceAccount<'info, TokenAccount>,

    pub associated_token_program: Program<'info, AssociatedToken>,
    pub token_program: Interface<'info, TokenInterface>,
    pub system_program: Program<'info, System>,
}

pub fn handle_create_event(
    context: Context<CreateEvent>,
    event_id: u64,
    description: String,
) -> Result<()> {
    require!(
        description.len() <= MAX_DESCRIPTION_LEN,
        BettingError::DescriptionTooLong
    );

    context.accounts.event.set_inner(Event {
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
    });

    context.accounts.config.event_count += 1;
    Ok(())
}
