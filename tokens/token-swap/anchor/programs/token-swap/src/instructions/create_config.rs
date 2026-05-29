use anchor_lang::prelude::*;

use crate::{
    constants::{BASIS_POINTS_DIVISOR, CONFIG_SEED},
    errors::*,
    state::Config,
};

pub fn handle_create_config(
    mut context: Context<CreateConfigAccounts>,
    fee: u16,
    admin_share_bps: u16,
) -> Result<()> {
    let bump = context.bumps.config;
    let config = &mut context.accounts.config;
    config.admin = context.accounts.admin.key();
    config.fee = fee;
    config.admin_share_bps = admin_share_bps;
    config.bump = bump;

    Ok(())
}

#[derive(Accounts)]
#[instruction(fee: u16, admin_share_bps: u16)]
pub struct CreateConfigAccounts<'info> {
    #[account(
        init,
        payer = payer,
        space = Config::DISCRIMINATOR.len() + Config::INIT_SPACE,
        seeds = [CONFIG_SEED],
        bump,
        constraint = (fee as u64) < BASIS_POINTS_DIVISOR @ AmmError::InvalidFee,
        constraint = (admin_share_bps as u64) < BASIS_POINTS_DIVISOR @ AmmError::AdminShareTooHigh,
    )]
    pub config: Account<'info, Config>,

    /// The admin of the AMM
    /// CHECK: Read only, delegatable creation
    pub admin: AccountInfo<'info>,

    /// The account paying for all rents
    #[account(mut)]
    pub payer: Signer<'info>,

    /// Solana ecosystem accounts
    pub system_program: Program<'info, System>,
}
