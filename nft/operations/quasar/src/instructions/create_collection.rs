use {
    crate::MintAuthorityPda,
    quasar_lang::{cpi::Seed, prelude::*},
    quasar_metadata::prelude::*,
    quasar_spl::prelude::*,
};

/// Accounts for creating a collection NFT.
///
/// The PDA `["authority"]` acts as mint authority and update authority.
#[derive(Accounts)]
pub struct CreateCollectionAccountConstraints {
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
    /// Metadata PDA - initialised by the Metaplex program.
    #[account(mut)]
    pub metadata: UncheckedAccount,
    /// Master edition PDA - initialised by the Metaplex program.
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

/// Creates a collection NFT with caller-supplied metadata: mints one token,
/// then creates the metadata account (with sized collection details) and the
/// master edition, all signed by the PDA authority.
#[inline(always)]
pub fn handle_create_collection(
    accounts: &mut CreateCollectionAccountConstraints,
    bumps: &CreateCollectionAccountConstraintsBumps,
    name: &str,
    symbol: &str,
    uri: &str,
) -> Result<(), ProgramError> {
    let bump = [bumps.mint_authority];
    let seeds: &[Seed] = &[
        Seed::from(b"authority" as &[u8]),
        Seed::from(&bump as &[u8]),
    ];

    // Mint 1 token (the collection NFT) to the destination.
    accounts
        .token_program
        .mint_to(
            &accounts.mint,
            &accounts.destination,
            &accounts.mint_authority,
            1u64,
        )
        .invoke_signed(seeds)?;
    log("Collection NFT minted!");

    // Create the metadata account, marked as a sized collection
    // (CollectionDetails::V1) so NFTs can be verified into it.
    super::create_metadata_account_v3(
        &accounts.token_metadata_program,
        &accounts.metadata,
        &accounts.mint,
        &accounts.mint_authority,
        &accounts.user,
        &accounts.mint_authority,
        &accounts.system_program,
        &accounts.rent,
        name,
        symbol,
        uri,
        accounts.mint_authority.address(),
        None,
        true,
    )?
    .invoke_signed(seeds)?;
    log("Metadata Account created!");

    // Create master edition.
    accounts
        .token_metadata_program
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
