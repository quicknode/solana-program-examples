use anchor_lang::prelude::*;

use crate::{
    constants::{BASIS_POINTS_DIVISOR, CONFIG_SEED},
    errors::*,
    state::Config,
};

pub fn handle_initialize_config(
    context: &mut Context<InitializeConfigAccountConstraints>,
    fee: u16,
    admin_share_bps: u16,
) -> Result<()> {
    let bump = context.bumps.config;
    let config = &mut context.accounts.config;
    config.admin = *context.accounts.admin.address();
    config.fee = fee;
    config.admin_share_bps = admin_share_bps;
    config.bump = bump;

    Ok(())
}

// The leading underscores are for rustc: `#[derive(Accounts)]` expands these
// into a path that never reads them, so the plain names warn as unused. The
// `constraint` expressions below are the real use.
#[derive(Accounts)]
#[instruction(_fee: u16, _admin_share_bps: u16)]
pub struct InitializeConfigAccountConstraints {
    #[account(
        init,
        payer = payer,
        space = Config::DISCRIMINATOR.len() + Config::INIT_SPACE,
        seeds = [CONFIG_SEED],
        bump,
        constraint = (_fee as u64) < BASIS_POINTS_DIVISOR @ AmmError::InvalidFee,
        constraint = (_admin_share_bps as u64) < BASIS_POINTS_DIVISOR @ AmmError::AdminShareTooHigh,
    )]
    pub config: BorshAccount<Config>,

    /// The admin of the AMM
    /// CHECK: Read only, delegatable creation
    pub admin: UncheckedAccount,

    /// The account paying for all rents
    #[account(mut)]
    pub payer: Signer,

    /// Solana ecosystem accounts
    pub system_program: Program<System>,
}
