use anchor_lang::prelude::*;
use anchor_lang::system_program::{create_account, CreateAccount};

declare_id!("ARVNCsYKDQsCLHbwUTJLpFXVrJdjhWZStyzvxmKe2xHi");

#[program]
pub mod create_system_account {
    use super::*;

    pub fn create_system_account(
        context: &mut Context<CreateSystemAccountAccountConstraints>,
    ) -> Result<()> {
        msg!("Program invoked. Creating a system account...");
        msg!(
            "  New public key will be: {}",
            context.accounts.new_account.address()
        );

        // The minimum lamports for rent exemption
        let lamports = Rent::get()?.try_minimum_balance(0)?;

        create_account(
            CpiContext::new(
                context.accounts.system_program.address(),
                CreateAccount {
                    from: context.accounts.payer.cpi_handle_mut(), // From pubkey
                    to: context.accounts.new_account.cpi_handle_mut(), // To pubkey
                },
            ),
            lamports,                                  // Lamports
            0,                                         // Space
            context.accounts.system_program.address(), // Owner Program
        )?;

        msg!("Account created successfully.");
        Ok(())
    }
}

#[derive(Accounts)]
pub struct CreateSystemAccountAccountConstraints {
    #[account(mut)]
    pub payer: Signer,
    #[account(mut)]
    pub new_account: Signer,
    pub system_program: Program<System>,
}
