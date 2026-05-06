use {
    crate::MintAuthorityPda,
    quasar_lang::prelude::*,
    quasar_metadata::prelude::*,
    quasar_spl::prelude::*,
};

/// Accounts for creating a collection NFT.
///
/// The PDA `["authority"]` acts as mint authority and update authority.
#[derive(Accounts)]
pub struct CreateCollection {
    #[account(mut)]
    pub user: Signer,
    #[account(
        mut,
        init,
        payer = user,
        mint(
            decimals = 0,
            authority = mint_authority,
            freeze_authority = Some(mint_authority),
            token_program = token_program,
        ),
    )]
    pub mint: Account<Mint>,
    /// PDA used as mint authority and update authority.
    #[account(address = MintAuthorityPda::seeds())]
    pub mint_authority: UncheckedAccount,
    /// Metadata PDA — initialised by the Metaplex program.
    #[account(mut)]
    pub metadata: UncheckedAccount,
    /// Master edition PDA — initialised by the Metaplex program.
    #[account(mut)]
    pub master_edition: UncheckedAccount,
    /// Token account to hold the collection NFT.
    #[account(
        mut,
        init(idempotent),
        payer = user,
        token(mint = mint, authority = user, token_program = token_program),
    )]
    pub destination: Account<Token>,
    pub system_program: Program<SystemProgram>,
    pub token_program: Program<TokenProgram>,
    pub token_metadata_program: Program<MetadataProgram>,
    pub rent: Sysvar<Rent>,
}

#[inline(always)]
pub fn handle_create_collection(accounts: &mut CreateCollection, bumps: &CreateCollectionBumps) -> Result<(), ProgramError> {
    let bump = [bumps.mint_authority];
    let seeds: &[Seed] = &[
        Seed::from(b"authority" as &[u8]),
        Seed::from(&bump as &[u8]),
    ];

    // Mint 1 token to the destination.
    accounts.token_program
        .mint_to(&accounts.mint, &accounts.destination, &accounts.mint_authority, 1u64)
        .invoke_signed(seeds)?;
    log("Collection NFT minted!");

    // Create metadata account.
    accounts.token_metadata_program
        .create_metadata_accounts_v3(
            &accounts.metadata,
            &accounts.mint,
            &accounts.mint_authority,
            &accounts.user,
            &accounts.mint_authority,
            &accounts.system_program,
            &accounts.rent,
            "DummyCollection",
            "DC",
            "",
            0,    // seller_fee_basis_points
            true, // is_mutable
            true, // update_authority_is_signer
        )?
        .invoke_signed(seeds)?;
    log("Metadata Account created!");

    // Create master edition.
    accounts.token_metadata_program
        .create_master_edition_v3(
            &accounts.master_edition,
            &accounts.mint,
            &accounts.mint_authority, // update_authority
            &accounts.mint_authority, // mint_authority
            &accounts.user,           // payer
            &accounts.metadata,
            &accounts.token_program,
            &accounts.system_program,
            &accounts.rent,
            Some(0), // max_supply = 0 means unique 1/1
        )
        .invoke_signed(seeds)?;
    log("Master Edition Account created");

    Ok(())
}
