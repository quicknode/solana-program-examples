use anchor_lang::prelude::*;
use anchor_spl::mint;
use anchor_spl::{
    token_2022::spl_token_2022::extension::permanent_delegate::PermanentDelegate,
    token_interface::{
        spl_pod::optional_keys::OptionalNonZeroPubkey,
        spl_token_2022::{
            extension::{BaseStateWithExtensions, StateWithExtensions},
            state::Mint as MintState,
        },
        Mint, Token2022,
    },
};

#[derive(Accounts)]
pub struct InitializeAccountConstraints {
    #[account(mut)]
    pub payer: Signer,

    #[account(
        init,
        payer = payer,
        mint::decimals = 2,
        mint::authority = payer,
        extensions::permanent_delegate::delegate = payer,
    )]
    pub mint_account: InterfaceAccount<Mint>,
    pub token_program: Program<Token2022>,
    pub system_program: Program<System>,
}

// helper to check mint data, and demonstrate how to read mint extension data within a program
fn check_mint_data(accounts: &mut InitializeAccountConstraints) -> Result<()> {
    let mint = &accounts.mint_account.cpi_handle_mut();
    let mint_data = mint.data.borrow();
    let mint_with_extension = StateWithExtensions::<MintState>::unpack(&mint_data)?;
    let extension_data = mint_with_extension.get_extension::<PermanentDelegate>()?;

    assert_eq!(
        extension_data.delegate,
        OptionalNonZeroPubkey::try_from(Some(accounts.payer.address()))?
    );

    msg!("{:?}", extension_data);
    Ok(())
}

pub fn handler(mut context: &mut Context<InitializeAccountConstraints>) -> Result<()> {
    check_mint_data(&mut context.accounts)?;
    Ok(())
}
