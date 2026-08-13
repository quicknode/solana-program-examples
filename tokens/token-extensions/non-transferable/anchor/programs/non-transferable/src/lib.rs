use anchor_lang::prelude::*;
use anchor_lang::system_program::{create_account, CreateAccount};
use anchor_spl::{
    token_2022::{
        initialize_mint2,
        spl_token_2022::{extension::ExtensionType, pod::PodMint},
        InitializeMint2,
    },
    token_interface::{non_transferable_mint_initialize, NonTransferableMintInitialize, Token2022},
};

declare_id!("8Bz4wpHaUckiC169Rg5ZfaBHFemp5S8RwTSDTKzhJ9W");

#[program]
pub mod non_transferable {
    use super::*;

    // There is currently not an anchor constraint to automatically initialize the NonTransferable extension
    // We can manually create and initialize the mint account via CPIs in the instruction handler
    pub fn initialize(context: &mut Context<InitializeAccountConstraints>) -> Result<()> {
        // Calculate space required for mint and extension data
        let mint_size =
            ExtensionType::try_calculate_account_len::<PodMint>(&[ExtensionType::NonTransferable])?;

        // Calculate minimum lamports required for size of mint account with extensions
        let lamports = Rent::get()?.try_minimum_balance(mint_size)?;

        // Invoke System Program to create new account with space for mint and extension data
        create_account(
            CpiContext::new(
                context.accounts.system_program.address(),
                CreateAccount {
                    from: context.accounts.payer.cpi_handle_mut(),
                    to: context.accounts.mint_account.cpi_handle_mut(),
                },
            ),
            lamports,                          // Lamports
            mint_size as u64,                  // Space
            &context.accounts.token_program.address(), // Owner Program
        )?;

        // Initialize the NonTransferable extension
        // This instruction must come before the instruction to initialize the mint data
        non_transferable_mint_initialize(CpiContext::new(
            context.accounts.token_program.address(),
            NonTransferableMintInitialize {
                mint: context.accounts.mint_account.cpi_handle_mut(),
            },
        ))?;

        // Initialize the standard mint account data
        initialize_mint2(
            CpiContext::new(
                context.accounts.token_program.address(),
                InitializeMint2 {
                    mint: context.accounts.mint_account.cpi_handle_mut(),
                },
            ),
            2,                               // decimals
            &context.accounts.payer.address(),       // mint authority
            Some(&context.accounts.payer.address()), // freeze authority
        )?;
        Ok(())
    }
}

#[derive(Accounts)]
pub struct InitializeAccountConstraints {
    #[account(mut)]
    pub payer: Signer,
    #[account(mut)]
    pub mint_account: Signer,

    pub token_program: Program<Token2022>,
    pub system_program: Program<System>,
}
