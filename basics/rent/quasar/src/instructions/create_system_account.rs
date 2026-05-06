use quasar_lang::{prelude::*, sysvars::Sysvar};

/// Accounts for creating a system account sized for address data.
#[derive(Accounts)]
pub struct CreateSystemAccount {
    #[account(mut)]
    pub payer: Signer,
    #[account(mut)]
    pub new_account: Signer,
    pub system_program: Program<SystemProgram>,
}

#[inline(always)]
pub fn handle_create_system_account(accounts: &mut CreateSystemAccount, name: &str, address: &str) -> Result<(), ProgramError> {
    // Calculate space needed for the serialised AddressData:
    // borsh-style: 4-byte length prefix + bytes for each String field.
    let space = 4 + name.len() + 4 + address.len();

    log("Program invoked. Creating a system account...");

    let system_program_address = Address::default();
    let rent = Rent::get()?;
    let lamports = rent.minimum_balance_unchecked(space);

    accounts.system_program
        .create_account(&accounts.payer, &accounts.new_account, lamports, space as u64, &system_program_address)
        .invoke()?;

    log("Account created successfully.");
    Ok(())
}
