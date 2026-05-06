use {
    crate::state::{MessageAccount, MessageAccountInner},
    quasar_lang::{prelude::*, sysvars::Sysvar},
};

/// Accounts for updating a message account.
/// Quasar's `set_inner` automatically handles realloc when the new message
/// is longer than the current account data. No explicit realloc needed.
#[derive(Accounts)]
pub struct Update {
    #[account(mut)]
    pub payer: Signer,
    #[account(mut)]
    pub message_account: Account<MessageAccount>,
    pub system_program: Program<SystemProgram>,
}

#[inline(always)]
pub fn handle_update(accounts: &mut Update, message: &str) -> Result<(), ProgramError> {
    let rent = Rent::get()?;
    accounts.message_account.set_inner(
        MessageAccountInner { message },
        accounts.payer.to_account_view(),
        rent.lamports_per_byte(),
        rent.exemption_threshold_raw(),
    )
}
