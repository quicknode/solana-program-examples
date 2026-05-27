use {
    crate::{state::{Config, ConfigInner}, ConfigPda},
    quasar_lang::prelude::*,
};

/// `Config` is a global singleton: one account per deployed program, derived
/// at the fixed seed `b"config"`. There is no `id` parameter — calling this
/// twice for the same program will fail because the account already exists.
#[derive(Accounts)]
pub struct CreateConfigAccounts {
    #[account(mut, init, payer = payer, address = ConfigPda::seeds())]
    pub config: Account<Config>,
    /// Admin authority for the AMM.
    pub admin: UncheckedAccount,
    #[account(mut)]
    pub payer: Signer,
    pub system_program: Program<SystemProgram>,
}

#[inline(always)]
pub fn handle_create_config(
    accounts: &mut CreateConfigAccounts,
    fee: u16,
    admin_share_bps: u16,
) -> Result<(), ProgramError> {
    if fee >= 10000 {
        return Err(ProgramError::InvalidArgument);
    }
    // `admin_share_bps` is the basis-points slice of the trading fee that
    // goes to the admin (rest goes to LPs). Anything >= 10_000 is nonsensical
    // (admin can't take more than the whole fee).
    if admin_share_bps >= 10000 {
        return Err(ProgramError::InvalidArgument);
    }
    accounts.config.set_inner(ConfigInner {
        admin: *accounts.admin.address(),
        fee: fee.into(),
        admin_share_bps: admin_share_bps.into(),
    });
    Ok(())
}
