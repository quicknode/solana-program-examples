use {
    crate::MintAuthorityPda,
    quasar_lang::{
        cpi::{CpiCall, InstructionAccount},
        prelude::*,
    },
    quasar_metadata::prelude::*,
};

/// Instruction discriminator of VerifySizedCollectionItem within the
/// Metaplex Token Metadata program - the verify instruction for sized
/// collections (the collection NFT carries `CollectionDetails::V1`).
const VERIFY_SIZED_COLLECTION_ITEM_DISCRIMINATOR: u8 = 30;

/// Accounts taken by VerifySizedCollectionItem.
const VERIFY_ACCOUNT_COUNT: usize = 6;

/// Accounts for verifying an NFT as part of a collection.
///
/// The Anchor version uses typed `MetadataAccount` / `MasterEditionAccount`
/// wrappers for owner and discriminant validation. In Quasar we use
/// `UncheckedAccount` and rely on the Metaplex program itself to validate
/// the accounts during CPI - the onchain program enforces correctness.
#[derive(Accounts)]
pub struct VerifyCollectionMintAccountConstraints {
    #[account(mut)]
    pub authority: Signer,
    /// The NFT's metadata account (will be updated with verified=true).
    #[account(mut)]
    pub metadata: UncheckedAccount,
    /// PDA used as collection authority.
    #[account(address = MintAuthorityPda::seeds())]
    pub mint_authority: UncheckedAccount,
    /// The collection mint.
    pub collection_mint: UncheckedAccount,
    /// The collection's metadata account. Writable: verifying a sized
    /// collection item increments the stored collection size.
    #[account(mut)]
    pub collection_metadata: UncheckedAccount,
    /// The collection's master edition account.
    pub collection_master_edition: UncheckedAccount,
    pub token_metadata_program: Program<MetadataProgram>,
}

/// Verifies the NFT's collection membership via a VerifySizedCollectionItem
/// CPI signed by the PDA collection authority.
///
/// The CPI is built here rather than with `quasar_metadata`'s
/// `verify_sized_collection_item` helper because the helper marks
/// `collection_metadata` readonly, while the Metaplex program writes the
/// incremented collection size to it.
#[inline(always)]
pub fn handle_verify_collection(
    accounts: &mut VerifyCollectionMintAccountConstraints,
    bumps: &VerifyCollectionMintAccountConstraintsBumps,
) -> Result<(), ProgramError> {
    let bump = [bumps.mint_authority];
    let seeds: &[Seed] = &[
        Seed::from(b"authority" as &[u8]),
        Seed::from(&bump as &[u8]),
    ];

    let metadata = accounts.metadata.to_account_view();
    let collection_authority = accounts.mint_authority.to_account_view();
    let payer = accounts.authority.to_account_view();
    let collection_mint = accounts.collection_mint.to_account_view();
    let collection_metadata = accounts.collection_metadata.to_account_view();
    let collection_master_edition = accounts.collection_master_edition.to_account_view();

    CpiCall::<VERIFY_ACCOUNT_COUNT, 1>::new(
        accounts.token_metadata_program.to_account_view().address(),
        [
            InstructionAccount::writable(metadata.address()),
            InstructionAccount::readonly_signer(collection_authority.address()),
            InstructionAccount::writable_signer(payer.address()),
            InstructionAccount::readonly(collection_mint.address()),
            InstructionAccount::writable(collection_metadata.address()),
            InstructionAccount::readonly(collection_master_edition.address()),
        ],
        [
            metadata,
            collection_authority,
            payer,
            collection_mint,
            collection_metadata,
            collection_master_edition,
        ],
        [VERIFY_SIZED_COLLECTION_ITEM_DISCRIMINATOR],
    )
    .invoke_signed(seeds)?;

    log("Collection Verified!");
    Ok(())
}
