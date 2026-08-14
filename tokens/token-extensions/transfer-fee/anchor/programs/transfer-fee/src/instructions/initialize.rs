use anchor_lang::prelude::*;
use anchor_lang::system_program::{create_account, CreateAccount};
use anchor_spl::{
    token_2022::{
        initialize_mint2,
        spl_token_2022::{
            extension::{
                transfer_fee::TransferFeeConfig, BaseStateWithExtensions, ExtensionType,
                StateWithExtensions,
            },
            pod::PodMint,
            state::Mint as MintState,
        },
        InitializeMint2,
    },
    token_interface::{
        spl_pod::optional_keys::OptionalNonZeroPubkey, transfer_fee_initialize, Token2022,
        TransferFeeInitialize,
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

// There is currently not an anchor constraint to automatically initialize the TransferFeeConfig extension
// We can manually create and initialize the mint account via CPIs in the instruction handler
pub fn handle_process_initialize(
    context: &mut Context<InitializeAccountConstraints>,
    transfer_fee_basis_points: u16,
    maximum_fee: u64,
) -> Result<()> {
    // Calculate space required for mint and extension data
    let mint_size =
        ExtensionType::try_calculate_account_len::<PodMint>(&[ExtensionType::TransferFeeConfig])?;

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
        lamports,                                  // Lamports
        mint_size as u64,                          // Space
        &context.accounts.token_program.address(), // Owner Program
    )?;

    // Initialize the transfer fee extension data
    // This instruction must come before the instruction to initialize the mint data
    transfer_fee_initialize(
        CpiContext::new(
            context.accounts.token_program.address(),
            TransferFeeInitialize {
                mint: context.accounts.mint_account.cpi_handle_mut(),
            },
        ),
        Some(&context.accounts.payer.address()), // transfer fee config authority (update fee)
        Some(&context.accounts.payer.address()), // withdraw authority (withdraw fees)
        transfer_fee_basis_points,               // transfer fee basis points (% fee per transfer)
        maximum_fee, // maximum fee (maximum units of token per transfer)
    )?;

    // Initialize the standard mint account data
    initialize_mint2(
        CpiContext::new(
            context.accounts.token_program.address(),
            InitializeMint2 {
                mint: context.accounts.mint_account.cpi_handle_mut(),
            },
        ),
        2,                                       // decimals
        &context.accounts.payer.address(),       // mint authority
        Some(&context.accounts.payer.address()), // freeze authority
    )?;

    handle_check_mint_data(&context.accounts)?;
    Ok(())
}

// helper to demonstrate how to read mint extension data within a program
pub fn handle_check_mint_data(accounts: &InitializeAccountConstraints) -> Result<()> {
    let mint = &accounts.mint_account.cpi_handle_mut();
    let mint_data = mint.data.borrow();
    let mint_with_extension = StateWithExtensions::<MintState>::unpack(&mint_data)?;
    let extension_data = mint_with_extension.get_extension::<TransferFeeConfig>()?;

    assert_eq!(
        extension_data.transfer_fee_config_authority,
        OptionalNonZeroPubkey::try_from(Some(accounts.payer.address()))?
    );

    assert_eq!(
        extension_data.withdraw_withheld_authority,
        OptionalNonZeroPubkey::try_from(Some(accounts.payer.address()))?
    );

    msg!("{:?}", extension_data);
    Ok(())
}
