use {
    crate::{
        error::AmmError,
        state::{Config, ConfigInner},
        ConfigPda, BASIS_POINTS_DIVISOR,
    },
    quasar_lang::prelude::*,
};

/// `Config` is a global singleton: one account per deployed program, derived
/// at the fixed seed `b"config"`. There is no `id` parameter - calling this
/// twice for the same program will fail because the account already exists.
#[derive(Accounts)]
pub struct InitializeConfigAccountConstraints {
    #[account(mut, init, payer = payer, address = ConfigPda::seeds())]
    pub config: Account<Config>,
    /// Admin authority for the AMM.
    pub admin: UncheckedAccount,
    #[account(mut)]
    pub payer: Signer,
    pub system_program: Program<SystemProgram>,
}

#[inline(always)]
pub fn handle_initialize_config(
    accounts: &mut InitializeConfigAccountConstraints,
    fee: u16,
    admin_share_bps: u16,
) -> Result<(), ProgramError> {
    require!((fee as u64) < BASIS_POINTS_DIVISOR, AmmError::InvalidFee);
    // `admin_share_bps` is the basis-points slice of the trading fee that
    // goes to the admin (rest goes to LPs). Anything >= 10_000 is nonsensical
    // (admin can't take more than the whole fee).
    require!(
        (admin_share_bps as u64) < BASIS_POINTS_DIVISOR,
        AmmError::AdminShareTooHigh
    );
    accounts.config.set_inner(ConfigInner {
        admin: *accounts.admin.address(),
        fee,
        admin_share_bps,
    });
    Ok(())
}
