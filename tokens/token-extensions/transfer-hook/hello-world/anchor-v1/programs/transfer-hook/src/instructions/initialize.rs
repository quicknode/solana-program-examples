use anchor_lang::prelude::*;
use anchor_spl::token_interface::{
    spl_pod::optional_keys::OptionalNonZeroPubkey,
    spl_token_2022::{
        extension::{
            transfer_hook::TransferHook as TransferHookExtension, BaseStateWithExtensions,
            StateWithExtensions,
        },
        state::Mint as MintState,
    },
    Mint, Token2022,
};

#[derive(Accounts)]
#[instruction(_decimals: u8)]
pub struct InitializeAccountConstraints<'info> {
    #[account(mut)]
    pub payer: Signer<'info>,

    #[account(
        init,
        payer = payer,
        mint::decimals = _decimals,
        mint::authority = payer,
        extensions::transfer_hook::authority = payer,
        extensions::transfer_hook::program_id = crate::ID
    )]
    pub mint_account: InterfaceAccount<'info, Mint>,
    pub token_program: Program<'info, Token2022>,
    pub system_program: Program<'info, System>,
}

// create a mint account that specifies this program as the transfer hook program
pub fn handler(mut context: Context<InitializeAccountConstraints>, _decimals: u8) -> Result<()> {
    handle_check_mint_data(&mut context.accounts)?;
    Ok(())
}

// helper to check mint data, and demonstrate how to read mint extension data within a program
fn handle_check_mint_data(accounts: &mut InitializeAccountConstraints) -> Result<()> {
    let mint = &accounts.mint_account.to_account_info();
    let mint_data = mint.data.borrow();
    // .map_err() needed because spl-token-2022 uses solana-program-error 2.x
    // while anchor-lang 1.0 uses 3.x - structurally identical but different semver types
    let mint_with_extension = StateWithExtensions::<MintState>::unpack(&mint_data)
        .map_err(|_| ProgramError::InvalidAccountData)?;
    let extension_data = mint_with_extension.get_extension::<TransferHookExtension>()
        .map_err(|_| ProgramError::InvalidAccountData)?;

    assert_eq!(
        extension_data.authority,
        OptionalNonZeroPubkey::try_from(Some(accounts.payer.key()))
            .map_err(|_| ProgramError::InvalidArgument)?
    );

    assert_eq!(
        extension_data.program_id,
        OptionalNonZeroPubkey::try_from(Some(crate::ID))
            .map_err(|_| ProgramError::InvalidArgument)?
    );

    msg!("{:?}", extension_data);
    Ok(())
}
