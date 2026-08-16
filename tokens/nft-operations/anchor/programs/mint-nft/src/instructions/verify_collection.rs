use anchor_lang::prelude::*;

// v2's anchor-spl wraps this CPI in terms of `CpiHandle`s, so the raw
// mpl-token-metadata `*Cpi` builder — which wants `&AccountInfo` — is not
// usable here. The collection is created sized (`CollectionDetails::V1`), so
// the sized-item variant is the matching instruction.
use anchor_spl::metadata::{
    verify_sized_collection_item, MasterEditionAccount, MetadataAccount,
    VerifySizedCollectionItem,
};
use anchor_spl::{metadata::Metadata, token::Mint};
// pinocchio does not re-export the instructions sysvar id; decode it here.
const INSTRUCTIONS_SYSVAR_ID: Address =
    anchor_lang::address!("Sysvar1nstructions1111111111111111111111111");

#[derive(Accounts)]
pub struct VerifyCollectionMintAccountConstraints {
    #[account(mut)]
    pub authority: Signer,
    #[account(mut)]
    pub metadata: MetadataAccount,
    pub mint: Account<Mint>,
    #[account(
        seeds = [b"authority"],
        bump,
    )]
    /// CHECK: This account is not initialized and is being used for signing purposes only
    pub mint_authority: UncheckedAccount,
    pub collection_mint: Account<Mint>,
    #[account(mut)]
    pub collection_metadata: MetadataAccount,
    pub collection_master_edition: MasterEditionAccount,
    pub system_program: Program<System>,
    #[account(address = INSTRUCTIONS_SYSVAR_ID)]
    /// CHECK: Sysvar instruction account that is being checked with an address constraint
    pub sysvar_instruction: UncheckedAccount,
    pub token_metadata_program: Program<Metadata>,
}

pub fn handle_verify_collection(
    accounts: &mut VerifyCollectionMintAccountConstraints,
    bumps: &VerifyCollectionMintAccountConstraintsBumps,
) -> Result<()> {
    let seeds = &[&b"authority"[..], &[bumps.mint_authority]];
    let signer_seeds = &[&seeds[..]];

    verify_sized_collection_item(
        CpiContext::new_with_signer(
            accounts.token_metadata_program.address(),
            VerifySizedCollectionItem {
                metadata: accounts.metadata.cpi_handle_mut(),
                collection_authority: accounts.mint_authority.cpi_handle(),
                payer: accounts.authority.cpi_handle_mut(),
                collection_mint: accounts.collection_mint.cpi_handle(),
                collection_metadata: accounts.collection_metadata.cpi_handle_mut(),
                collection_master_edition: accounts.collection_master_edition.cpi_handle(),
            },
            signer_seeds,
        ),
        None,
    )?;

    msg!("Collection Verified!");

    Ok(())
}
