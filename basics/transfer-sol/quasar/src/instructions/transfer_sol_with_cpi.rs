use quasar_lang::prelude::*;

/// Accounts for transferring SOL via system program CPI.
#[derive(Accounts)]
pub struct TransferSolWithCpi {
    #[account(mut)]
    pub payer: Signer,
    #[account(mut)]
    pub recipient: UncheckedAccount,
    pub system_program: Program<SystemProgram>,
}

#[inline(always)]
pub fn handle_transfer_sol_with_cpi(accounts: &mut TransferSolWithCpi, amount: u64) -> Result<(), ProgramError> {
    accounts.system_program
        .transfer(&accounts.payer, &accounts.recipient, amount)
        .invoke()
}
