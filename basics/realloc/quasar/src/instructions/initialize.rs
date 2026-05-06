use {
    crate::state::{MessageAccount, MessageAccountInner},
    quasar_lang::{prelude::*, sysvars::Sysvar},
};

/// Accounts for initialising a new message account.
/// The message_account is a random keypair (not a PDA) — same as the Anchor version.
#[derive(Accounts)]
pub struct Initialize {
    #[account(mut)]
    pub payer: Signer,
    #[account(mut, init, payer = payer)]
    pub message_account: Account<MessageAccount>,
    pub system_program: Program<SystemProgram>,
}

#[inline(always)]
pub fn handle_initialize(accounts: &mut Initialize, message: &str) -> Result<(), ProgramError> {
    let rent = Rent::get()?;
    accounts.message_account.set_inner(
        MessageAccountInner { message },
        accounts.payer.to_account_view(),
        rent.lamports_per_byte(),
        rent.exemption_threshold_raw(),
    )
}
