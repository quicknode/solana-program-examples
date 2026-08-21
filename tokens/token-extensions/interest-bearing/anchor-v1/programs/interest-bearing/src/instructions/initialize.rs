use anchor_lang::prelude::*;
use anchor_lang::system_program::{create_account, CreateAccount};
use anchor_spl::{
    token_2022::{
        initialize_mint2,
        spl_token_2022::{extension::ExtensionType, pod::PodMint},
        InitializeMint2,
    },
    token_interface::{interest_bearing_mint_initialize, InterestBearingMintInitialize, Token2022},
};

use crate::check_mint_data;

#[derive(Accounts)]
pub struct InitializeAccountConstraints<'info> {
    #[account(mut)]
    pub payer: Signer<'info>,
    #[account(mut)]
    pub mint_account: Signer<'info>,

    pub token_program: Program<'info, Token2022>,
    pub system_program: Program<'info, System>,
}

pub fn handler(context: Context<InitializeAccountConstraints>, rate: i16) -> Result<()> {
    // Calculate space required for mint and extension data
    let mint_size = ExtensionType::try_calculate_account_len::<PodMint>(&[
        ExtensionType::InterestBearingConfig,
    ])?;

    // Calculate minimum lamports required for size of mint account with extensions
    let lamports = (Rent::get()?).minimum_balance(mint_size);

    // Invoke System Program to create new account with space for mint and extension data
    create_account(
        CpiContext::new(
            context.accounts.system_program.key(),
            CreateAccount {
                from: context.accounts.payer.to_account_info(),
                to: context.accounts.mint_account.to_account_info(),
            },
        ),
        lamports,                          // Lamports
        mint_size as u64,                  // Space
        &context.accounts.token_program.key(), // Owner Program
    )?;

    // Initialize the InterestBearingConfig extension
    // This instruction must come before the instruction to initialize the mint data
    interest_bearing_mint_initialize(
        CpiContext::new(
            context.accounts.token_program.key(),
            InterestBearingMintInitialize {
                token_program_id: context.accounts.token_program.to_account_info(),
                mint: context.accounts.mint_account.to_account_info(),
            },
        ),
        Some(context.accounts.payer.key()),
        rate,
    )?;

    // Initialize the standard mint account data
    initialize_mint2(
        CpiContext::new(
            context.accounts.token_program.key(),
            InitializeMint2 {
                mint: context.accounts.mint_account.to_account_info(),
            },
        ),
        2,                               // decimals
        &context.accounts.payer.key(),       // mint authority
        Some(&context.accounts.payer.key()), // freeze authority
    )?;

    check_mint_data(
        &context.accounts.mint_account.to_account_info(),
        &context.accounts.payer.key(),
    )?;
    Ok(())
}
