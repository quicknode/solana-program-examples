use {
    crate::state::{PowerStatus, PowerStatusInner},
    quasar_lang::prelude::*,
};

/// Accounts for initialising the power status (PDA seeded by "power").
#[derive(Accounts)]
pub struct InitializeLever {
    #[account(mut)]
    pub payer: Signer,
    #[account(mut, init, payer = payer, address = PowerStatus::seeds())]
    pub power: Account<PowerStatus>,
    pub system_program: Program<SystemProgram>,
}

#[inline(always)]
pub fn handle_initialize(accounts: &mut InitializeLever) -> Result<(), ProgramError> {
    // Power starts off (false). Counter-style fixed-size set_inner takes only the inner value.
    accounts.power.set_inner(PowerStatusInner { is_on: PodBool::from(false) });
    Ok(())
}
