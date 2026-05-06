use quasar_lang::prelude::*;

/// PDA seed marker for the rent-vault account. With the new derive grammar
/// (`address = <expr>`) we need a `Seeds` impl to validate the address;
/// `seeds = [b"rent_vault"]` is no longer accepted.
#[derive(Seeds)]
#[seeds(b"rent_vault")]
pub struct RentVault;

/// Accounts for funding the rent vault PDA.
/// Transfers lamports from the payer to the vault via system program CPI.
/// When lamports are sent to a new address, the system program creates
/// a system-owned account automatically.
#[derive(Accounts)]
pub struct InitRentVault {
    #[account(mut)]
    pub payer: Signer,
    #[account(mut, address = RentVault::seeds())]
    pub rent_vault: UncheckedAccount,
    pub system_program: Program<SystemProgram>,
}

#[inline(always)]
pub fn handle_init_rent_vault(accounts: &mut InitRentVault, fund_lamports: u64) -> Result<(), ProgramError> {
    accounts.system_program
        .transfer(&accounts.payer, &accounts.rent_vault, fund_lamports)
        .invoke()
}
