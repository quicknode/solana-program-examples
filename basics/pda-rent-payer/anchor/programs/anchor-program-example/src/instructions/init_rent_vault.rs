use anchor_lang::prelude::*;
use anchor_lang::system_program::{transfer, Transfer};

#[derive(Accounts)]
pub struct InitRentVaultAccountConstraints {
    #[account(mut)]
    payer: Signer,

    #[account(
        mut,
        seeds = [
            b"rent_vault",
        ],
        bump,
    )]
    rent_vault: SystemAccount,
    system_program: Program<System>,
}

// When lamports are transferred to a new address (without and existing account),
// An account owned by the system program is created by default
pub fn handle_init_rent_vault(
    context: &mut Context<InitRentVaultAccountConstraints>,
    fund_lamports: u64,
) -> Result<()> {
    transfer(
        CpiContext::new(
            context.accounts.system_program.address(),
            Transfer {
                from: context.accounts.payer.cpi_handle_mut(),
                to: context.accounts.rent_vault.cpi_handle_mut(),
            },
        ),
        fund_lamports,
    )?;
    Ok(())
}
