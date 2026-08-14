use anchor_lang::prelude::*;
use anchor_lang::system_program::{create_account, CreateAccount};
use anchor_spl::{
    token_2022::{
        initialize_mint2,
        spl_token_2022::{
            extension::{permanent_delegate::PermanentDelegate, ExtensionType},
            pod::PodMint,
        },
        InitializeMint2,
    },
    token_2022_extensions::permanent_delegate::{
        permanent_delegate_initialize, PermanentDelegateInitialize,
    },
    token_interface::{
        spl_pod::optional_keys::OptionalNonZeroPubkey,
        spl_token_2022::{
            extension::{BaseStateWithExtensions, StateWithExtensions},
            state::Mint as MintState,
        },
        Token2022,
    },
};

#[derive(Accounts)]
pub struct InitializeAccountConstraints {
    #[account(mut)]
    pub payer: Signer,

    #[account(mut)]
    pub mint_account: Signer,

    pub token_program: Program<Token2022>,
    pub system_program: Program<System>,
}

// helper to check mint data, and demonstrate how to read mint extension data within a program
fn check_mint_data(context: &Context<InitializeAccountConstraints>) -> Result<()> {
    let mint = context.accounts.mint_account.account();
    let mint_data = mint.try_borrow()?;
    let mint_with_extension = StateWithExtensions::<MintState>::unpack(&mint_data)?;
    let extension_data = mint_with_extension.get_extension::<PermanentDelegate>()?;

    assert_eq!(
        extension_data.delegate,
        OptionalNonZeroPubkey::try_from(Some(*context.accounts.payer.address()))?
    );

    msg!("{:?}", extension_data);
    Ok(())
}

// There is currently not an anchor constraint to automatically initialize the
// PermanentDelegate extension. We can manually create and initialize the mint
// account via CPIs in the instruction handler.
pub fn handler(context: &mut Context<InitializeAccountConstraints>) -> Result<()> {
    // Calculate space required for mint and extension data
    let mint_size =
        ExtensionType::try_calculate_account_len::<PodMint>(&[ExtensionType::PermanentDelegate])?;

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
        lamports,                                 // Lamports
        mint_size as u64,                         // Space
        context.accounts.token_program.address(), // Owner Program
    )?;

    // Initialize the PermanentDelegate extension
    // This instruction must come before the instruction to initialize the mint data
    permanent_delegate_initialize(
        CpiContext::new(
            context.accounts.token_program.address(),
            PermanentDelegateInitialize {
                mint: context.accounts.mint_account.cpi_handle_mut(),
            },
        ),
        context.accounts.payer.address(),
    )?;

    // Initialize the standard mint account data
    initialize_mint2(
        CpiContext::new(
            context.accounts.token_program.address(),
            InitializeMint2 {
                mint: context.accounts.mint_account.cpi_handle_mut(),
            },
        ),
        2,                                // decimals
        context.accounts.payer.address(), // mint authority
        None,                             // freeze authority
    )?;

    check_mint_data(context)?;
    Ok(())
}
