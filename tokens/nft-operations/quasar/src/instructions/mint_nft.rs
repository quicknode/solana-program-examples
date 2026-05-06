use {
    crate::MintAuthorityPda,
    quasar_lang::prelude::*,
    quasar_metadata::prelude::*,
    quasar_spl::prelude::*,
};

/// Accounts for minting an individual NFT with a collection reference.
#[derive(Accounts)]
pub struct MintNft {
    #[account(mut)]
    pub owner: Signer,
    #[account(
        mut,
        init,
        payer = owner,
        mint(
            decimals = 0,
            authority = mint_authority,
            freeze_authority = Some(mint_authority),
            token_program = token_program,
        ),
    )]
    pub mint: Account<Mint>,
    /// Token account to hold the NFT.
    #[account(
        mut,
        init(idempotent),
        payer = owner,
        token(mint = mint, authority = owner, token_program = token_program),
    )]
    pub destination: Account<Token>,
    /// Metadata PDA — initialised by the Metaplex program.
    #[account(mut)]
    pub metadata: UncheckedAccount,
    /// Master edition PDA — initialised by the Metaplex program.
    #[account(mut)]
    pub master_edition: UncheckedAccount,
    /// PDA used as mint authority and update authority.
    #[account(address = MintAuthorityPda::seeds())]
    pub mint_authority: UncheckedAccount,
    /// The collection mint (must already exist).
    #[account(mut)]
    pub collection_mint: Account<Mint>,
    pub system_program: Program<SystemProgram>,
    pub token_program: Program<TokenProgram>,
    pub token_metadata_program: Program<MetadataProgram>,
    pub rent: Sysvar<Rent>,
}

#[inline(always)]
pub fn handle_mint_nft(accounts: &mut MintNft, bumps: &MintNftBumps) -> Result<(), ProgramError> {
    let bump = [bumps.mint_authority];
    let seeds: &[Seed] = &[
        Seed::from(b"authority" as &[u8]),
        Seed::from(&bump as &[u8]),
    ];

    // Mint 1 token to the destination.
    accounts.token_program
        .mint_to(&accounts.mint, &accounts.destination, &accounts.mint_authority, 1u64)
        .invoke_signed(seeds)?;
    log("NFT minted!");

    // Create metadata with collection reference.
    // Note: The collection is set as unverified here; call verify_collection
    // separately to verify it.
    accounts.token_metadata_program
        .create_metadata_accounts_v3(
            &accounts.metadata,
            &accounts.mint,
            &accounts.mint_authority,
            &accounts.owner,
            &accounts.mint_authority,
            &accounts.system_program,
            &accounts.rent,
            "Mint Test",
            "YAY",
            "",
            0,    // seller_fee_basis_points
            true, // is_mutable
            true, // update_authority_is_signer
        )?
        .invoke_signed(seeds)?;

    // Create master edition.
    accounts.token_metadata_program
        .create_master_edition_v3(
            &accounts.master_edition,
            &accounts.mint,
            &accounts.mint_authority, // update_authority
            &accounts.mint_authority, // mint_authority
            &accounts.owner,          // payer
            &accounts.metadata,
            &accounts.token_program,
            &accounts.system_program,
            &accounts.rent,
            Some(0), // max_supply = 0 means unique 1/1
        )
        .invoke_signed(seeds)?;

    Ok(())
}
