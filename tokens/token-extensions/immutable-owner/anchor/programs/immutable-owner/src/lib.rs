use anchor_lang::prelude::*;
use anchor_lang::system_program::{create_account, CreateAccount};
use anchor_spl::{
    token_2022::{
        initialize_account3,
        spl_token_2022::{extension::ExtensionType, pod::PodAccount},
        InitializeAccount3,
    },
    token_interface::{immutable_owner_initialize, ImmutableOwnerInitialize, Mint, Token2022},
};

declare_id!("6g5URpqqurW8RbKjuGeRCVZBKky3J4kYcLeotQ6vj6UT");

#[program]
pub mod immutable_owner {
    use super::*;

    // There is currently not an anchor constraint to automatically initialize the ImmutableOwner extension
    // We can manually create and initialize the token account via CPIs in the instruction handler
    pub fn initialize(context: &mut Context<InitializeAccountConstraints>) -> Result<()> {
        // Calculate space required for token and extension data
        let token_account_size = ExtensionType::try_calculate_account_len::<PodAccount>(&[
            ExtensionType::ImmutableOwner,
        ])?;

        // Calculate minimum lamports required for size of token account with extensions
        let lamports = Rent::get()?.try_minimum_balance(token_account_size)?;

        // Invoke System Program to create new account with space for token account and extension data
        create_account(
            CpiContext::new(
                context.accounts.system_program.address(),
                CreateAccount {
                    from: context.accounts.payer.cpi_handle_mut(),
                    to: context.accounts.token_account.cpi_handle_mut(),
                },
            ),
            lamports,                                  // Lamports
            token_account_size as u64,                 // Space
            &context.accounts.token_program.address(), // Owner Program
        )?;

        // Initialize the token account with the immutable owner extension
        immutable_owner_initialize(CpiContext::new(
            context.accounts.token_program.address(),
            ImmutableOwnerInitialize {
                token_account: context.accounts.token_account.cpi_handle_mut(),
            },
        ))?;

        // Initialize the standard token account data
        initialize_account3(CpiContext::new(
            context.accounts.token_program.address(),
            InitializeAccount3 {
                account: context.accounts.token_account.cpi_handle_mut(),
                mint: context.accounts.mint_account.cpi_handle(),
                authority: context.accounts.payer.cpi_handle(),
            },
        ))?;
        Ok(())
    }
}

#[derive(Accounts)]
pub struct InitializeAccountConstraints {
    #[account(mut)]
    pub payer: Signer,

    #[account(mut)]
    pub token_account: Signer,
    pub mint_account: InterfaceAccount<Mint>,
    pub token_program: Program<Token2022>,
    pub system_program: Program<System>,
}
