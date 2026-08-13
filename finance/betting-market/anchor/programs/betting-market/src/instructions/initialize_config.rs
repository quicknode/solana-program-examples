use anchor_lang::prelude::*;
use anchor_spl::mint;
use anchor_spl::token_interface::{Mint, TokenInterface};

use crate::{error::BettingError, Config};

pub const MAX_FEE_BPS: u16 = 10_000;

#[derive(Accounts)]
pub struct InitializeConfigAccountConstraints {
    #[account(mut)]
    pub admin: Signer,

    #[account(mint::token_program = token_program)]
    pub token_mint: InterfaceAccount<Mint>,

    #[account(
        init,
        payer = admin,
        space = Config::DISCRIMINATOR.len() + Config::INIT_SPACE,
        seeds = [b"config"],
        bump
    )]
    pub config: BorshAccount<Config>,

    pub token_program: Interface<'static, TokenInterface>,
    pub system_program: Program<System>,
}

pub fn handle_initialize_config(
    context: &mut Context<InitializeConfigAccountConstraints>,
    fee_bps: u16,
    fee_recipient: Address,
) -> Result<()> {
    require!(fee_bps <= MAX_FEE_BPS, BettingError::FeeTooHigh);

    *context.accounts.config = (Config {
        admin: *context.accounts.admin.address(),
        token_mint: *context.accounts.token_mint.address(),
        fee_recipient,
        fee_bps,
        event_count: 0,
        bump: context.bumps.config,
    });
    Ok(())
}
