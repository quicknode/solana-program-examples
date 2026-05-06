use {
    crate::instructions::init_rent_vault::RentVault,
    quasar_lang::{prelude::*, sysvars::Sysvar},
};

/// Accounts for creating a new account funded by the rent vault PDA.
/// The rent vault signs the create_account CPI via PDA seeds.
#[derive(Accounts)]
pub struct CreateNewAccount {
    #[account(mut)]
    pub new_account: Signer,
    #[account(mut, address = RentVault::seeds())]
    pub rent_vault: UncheckedAccount,
    pub system_program: Program<SystemProgram>,
}

#[inline(always)]
pub fn handle_create_new_account(accounts: &mut CreateNewAccount, rent_vault_bump: u8) -> Result<(), ProgramError> {
    // Build PDA signer seeds: ["rent_vault", bump].
    let bump_bytes = [rent_vault_bump];
    let seeds: &[Seed] = &[
        Seed::from(b"rent_vault" as &[u8]),
        Seed::from(&bump_bytes as &[u8]),
    ];

    let system_program_address = Address::default();
    let rent = Rent::get()?;
    let lamports = rent.minimum_balance_unchecked(0);

    accounts.system_program
        .create_account(&accounts.rent_vault, &accounts.new_account, lamports, 0u64, &system_program_address)
        .invoke_signed(seeds)
}
