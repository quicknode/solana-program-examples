use anchor_lang::prelude::*;
use anchor_spl::token_interface::{
    spl_pod::optional_keys::OptionalNonZeroPubkey, token_metadata_update_authority, Mint,
    Token2022, TokenMetadataUpdateAuthority,
};

#[derive(Accounts)]
pub struct UpdateAuthorityAccountConstraints {
    pub current_authority: Signer,
    pub new_authority: Option<UncheckedAccount>,

    #[account(
        mut,
        extensions::metadata_pointer::metadata_address = mint_account,
    )]
    pub mint_account: InterfaceAccount<Mint>,
    pub token_program: Program<Token2022>,
    pub system_program: Program<System>,
}

pub fn process_update_authority(context: &mut Context<UpdateAuthorityAccountConstraints>) -> Result<()> {
    let new_authority_key = match &context.accounts.new_authority {
        Some(account) => OptionalNonZeroPubkey::try_from(Some(account.address()))?,
        None => OptionalNonZeroPubkey::try_from(None)?,
    };

    // Change update authority
    token_metadata_update_authority(
        CpiContext::new(
            context.accounts.token_program.address(),
            TokenMetadataUpdateAuthority {
                program_id: context.accounts.token_program.cpi_handle_mut(),
                metadata: context.accounts.mint_account.cpi_handle_mut(),
                current_authority: context.accounts.current_authority.cpi_handle(),

                // new authority isn't actually needed as account in the CPI
                // using current_authority as a placeholder to satisfy the struct
                new_authority: context.accounts.current_authority.cpi_handle_mut(),
            },
        ),
        new_authority_key,
    )?;
    Ok(())
}
