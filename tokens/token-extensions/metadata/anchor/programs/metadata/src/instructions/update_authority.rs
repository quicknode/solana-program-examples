use anchor_lang::prelude::*;
use anchor_spl::token_interface::{
    token_metadata_update_authority, Mint, Token2022, TokenMetadataUpdateAuthority,
};

#[derive(Accounts)]
pub struct UpdateAuthorityAccountConstraints {
    pub current_authority: Signer,
    pub new_authority: Option<UncheckedAccount>,

    // v2 has no `extensions::*` validation constraint; Token-2022 checks the
    // metadata pointer and the update authority itself when the CPI runs.
    #[account(mut)]
    pub mint_account: InterfaceAccount<Mint>,
    pub token_program: Program<Token2022>,
    pub system_program: Program<System>,
}

pub fn process_update_authority(
    context: &mut Context<UpdateAuthorityAccountConstraints>,
) -> Result<()> {
    // v2 takes the new authority as a plain `Option<&Address>` and does the
    // `OptionalNonZeroPubkey` conversion itself.
    let new_authority = context
        .accounts
        .new_authority
        .as_ref()
        .map(|account| *account.address());

    // Change update authority. v2's struct drops the program-id and
    // new-authority slots: neither is passed as an account.
    token_metadata_update_authority(
        CpiContext::new(
            context.accounts.token_program.address(),
            TokenMetadataUpdateAuthority {
                metadata: context.accounts.mint_account.cpi_handle_mut(),
                current_authority: context.accounts.current_authority.cpi_handle(),
            },
        ),
        new_authority.as_ref(),
    )?;
    Ok(())
}
