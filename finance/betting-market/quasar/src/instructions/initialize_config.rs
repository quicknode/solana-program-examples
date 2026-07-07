use quasar_lang::prelude::*;
use quasar_spl::prelude::*;

use crate::errors::BettingError;
use crate::state::{Config, ConfigInner};

pub const MAX_FEE_BPS: u16 = 10_000;

#[derive(Accounts)]
pub struct InitializeConfigAccountConstraints {
    #[account(mut)]
    pub admin: Signer,

    pub token_mint: Account<Mint>,

    #[account(init, payer = admin, address = Config::seeds())]
    pub config: Account<Config>,

    pub rent: Sysvar<Rent>,
    pub token_program: Program<TokenProgram>,
    pub system_program: Program<SystemProgram>,
}

#[inline(always)]
pub fn handle_initialize_config(
    accounts: &mut InitializeConfigAccountConstraints,
    fee_bps: u16,
    fee_recipient: Address,
    bumps: &InitializeConfigAccountConstraintsBumps,
) -> Result<(), ProgramError> {
    require!(fee_bps <= MAX_FEE_BPS, BettingError::FeeTooHigh);

    accounts.config.set_inner(ConfigInner {
        admin: *accounts.admin.address(),
        token_mint: *accounts.token_mint.address(),
        fee_recipient,
        fee_bps,
        event_count: 0,
        bump: bumps.config,
    });
    Ok(())
}
