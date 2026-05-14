use {
    quasar_lang::prelude::*,
    quasar_metadata::prelude::*,
    quasar_spl::prelude::*,
};

/// Accounts for creating a new token mint with Metaplex metadata.
///
/// The mint is initialised via Quasar's `#[account(init)]`. The metadata
/// PDA is created by an explicit CPI to the Metaplex Token Metadata program
/// because the new `metadata(...)` derive behaviour only accepts compile-time
/// constants for `name` / `symbol` / `uri`; this instruction takes them at
/// runtime.
#[derive(Accounts)]
pub struct CreateToken {
    #[account(mut)]
    pub payer: Signer,
    #[account(
        mut,
        init,
        payer = payer,
        mint(
            decimals = 9,
            authority = payer,
            freeze_authority = Some(payer),
            token_program = token_program,
        ),
    )]
    pub mint_account: Account<Mint>,
    /// The metadata PDA — will be initialised by the Metaplex program.
    #[account(mut)]
    pub metadata_account: UncheckedAccount,
    pub token_program: Program<TokenProgram>,
    pub token_metadata_program: Program<MetadataProgram>,
    pub system_program: Program<SystemProgram>,
    pub rent: Sysvar<Rent>,
}

#[inline(always)]
pub fn handle_create_token(
    accounts: &mut CreateToken,
    token_name: &str,
    token_symbol: &str,
    token_uri: &str,
) -> Result<(), ProgramError> {
    log("Creating metadata account");

    accounts.token_metadata_program
        .create_metadata_accounts_v3(
            &accounts.metadata_account,
            &accounts.mint_account,
            &accounts.payer, // mint_authority
            &accounts.payer, // payer
            &accounts.payer, // update_authority
            &accounts.system_program,
            &accounts.rent,
            token_name,
            token_symbol,
            token_uri,
            0,     // seller_fee_basis_points
            false, // is_mutable
            true,  // update_authority_is_signer
        )?
        .invoke()?;

    log("Token created successfully.");
    Ok(())
}
