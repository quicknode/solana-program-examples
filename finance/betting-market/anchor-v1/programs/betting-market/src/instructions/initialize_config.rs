use anchor_lang::prelude::*;
use anchor_spl::token_interface::{Mint, TokenInterface};

use crate::{error::BettingError, Config};

pub const MAX_FEE_BPS: u16 = 10_000;

#[derive(Accounts)]
pub struct InitializeConfigAccountConstraints<'info> {
    #[account(mut)]
    pub admin: Signer<'info>,

    #[account(mint::token_program = token_program)]
    pub token_mint: InterfaceAccount<'info, Mint>,

    #[account(
        init,
        payer = admin,
        space = Config::DISCRIMINATOR.len() + Config::INIT_SPACE,
        seeds = [b"config"],
        bump
    )]
    pub config: Account<'info, Config>,

    pub token_program: Interface<'info, TokenInterface>,
    pub system_program: Program<'info, System>,
}

pub fn handle_initialize_config(
    context: Context<InitializeConfigAccountConstraints>,
    fee_bps: u16,
    fee_recipient: Pubkey,
) -> Result<()> {
    require!(fee_bps <= MAX_FEE_BPS, BettingError::FeeTooHigh);

    context.accounts.config.set_inner(Config {
        admin: context.accounts.admin.key(),
        token_mint: context.accounts.token_mint.key(),
        fee_recipient,
        fee_bps,
        event_count: 0,
        bump: context.bumps.config,
    });
    Ok(())
}
