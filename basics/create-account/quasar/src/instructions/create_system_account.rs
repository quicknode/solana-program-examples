use quasar_lang::{prelude::*, sysvars::Sysvar};

/// Accounts for creating a new system-owned account.
/// Both payer and new_account must sign the transaction.
#[derive(Accounts)]
pub struct CreateSystemAccount {
    #[account(mut)]
    pub payer: Signer,
    #[account(mut)]
    pub new_account: Signer,
    pub system_program: Program<SystemProgram>,
}

#[inline(always)]
pub fn handle_create_system_account(
    accounts: &mut CreateSystemAccount,
) -> Result<(), ProgramError> {
    let system_program_address = Address::default();
    let rent = Rent::get()?;
    let lamports = rent.minimum_balance_unchecked(0);
    accounts
        .system_program
        .create_account(&accounts.payer, &accounts.new_account, lamports, 0u64, &system_program_address)
        .invoke()
}
