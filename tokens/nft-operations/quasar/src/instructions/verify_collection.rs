use {
    crate::MintAuthorityPda,
    quasar_lang::prelude::*,
    quasar_metadata::prelude::*,
};

/// Accounts for verifying an NFT as part of a collection.
///
/// Uses `verify_sized_collection_item` which is the Metaplex Token Metadata
/// instruction for verifying collection membership on sized collections.
///
/// The Anchor version uses typed `MetadataAccount` / `MasterEditionAccount`
/// wrappers for owner and discriminant validation. In Quasar we use
/// `UncheckedAccount` and rely on the Metaplex program itself to validate
/// the accounts during CPI — the onchain program enforces correctness.
#[derive(Accounts)]
pub struct VerifyCollectionMint {
    pub authority: Signer,
    /// The NFT's metadata account (will be updated with verified=true).
    #[account(mut)]
    pub metadata: UncheckedAccount,
    /// PDA used as collection authority.
    #[account(address = MintAuthorityPda::seeds())]
    pub mint_authority: UncheckedAccount,
    /// The collection mint.
    pub collection_mint: UncheckedAccount,
    /// The collection's metadata account.
    #[account(mut)]
    pub collection_metadata: UncheckedAccount,
    /// The collection's master edition account.
    pub collection_master_edition: UncheckedAccount,
    pub system_program: Program<SystemProgram>,
    pub token_metadata_program: Program<MetadataProgram>,
}

#[inline(always)]
pub fn handle_verify_collection(accounts: &mut VerifyCollectionMint, bumps: &VerifyCollectionMintBumps) -> Result<(), ProgramError> {
    let bump = [bumps.mint_authority];
    let seeds: &[Seed] = &[
        Seed::from(b"authority" as &[u8]),
        Seed::from(&bump as &[u8]),
    ];

    accounts.token_metadata_program
        .verify_sized_collection_item(
            &accounts.metadata,
            &accounts.mint_authority,
            &accounts.authority, // payer
            &accounts.collection_mint,
            &accounts.collection_metadata,
            &accounts.collection_master_edition,
        )
        .invoke_signed(seeds)?;

    log("Collection Verified!");
    Ok(())
}
