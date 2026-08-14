use anchor_lang::prelude::*;

use anchor_spl::metadata::mpl_token_metadata::instructions::{
    VerifyCollectionV1Cpi, VerifyCollectionV1CpiAccounts,
};
use anchor_spl::metadata::{MasterEditionAccount, MetadataAccount};
use anchor_spl::{metadata::Metadata, token::Mint};
// In Anchor 1.0, sysvar::instructions::ID moved - use the well-known address directly
const INSTRUCTIONS_SYSVAR_ID: Address =
    anchor_lang::solana_program::pubkey::pubkey!("Sysvar1nstructions1111111111111111111111111");

#[derive(Accounts)]
pub struct VerifyCollectionMintAccountConstraints {
    pub authority: Signer,
    #[account(mut)]
    pub metadata: Account<MetadataAccount>,
    pub mint: Account<Mint>,
    #[account(
        seeds = [b"authority"],
        bump,
    )]
    /// CHECK: This account is not initialized and is being used for signing purposes only
    pub mint_authority: UncheckedAccount,
    pub collection_mint: Account<Mint>,
    #[account(mut)]
    pub collection_metadata: Account<MetadataAccount>,
    pub collection_master_edition: Account<MasterEditionAccount>,
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
    let metadata = &accounts.metadata.cpi_handle_mut();
    let authority = &accounts.mint_authority.cpi_handle_mut();
    let collection_mint = &accounts.collection_mint.cpi_handle_mut();
    let collection_metadata = &accounts.collection_metadata.cpi_handle_mut();
    let collection_master_edition = &accounts.collection_master_edition.cpi_handle_mut();
    let system_program = &accounts.system_program.cpi_handle_mut();
    let sysvar_instructions = &accounts.sysvar_instruction.cpi_handle_mut();
    let spl_metadata_program = &accounts.token_metadata_program.cpi_handle_mut();

    let seeds = &[&b"authority"[..], &[bumps.mint_authority]];
    let signer_seeds = &[&seeds[..]];

    let verify_collection = VerifyCollectionV1Cpi::new(
        spl_metadata_program,
        VerifyCollectionV1CpiAccounts {
            authority,
            delegate_record: None,
            metadata,
            collection_mint,
            collection_metadata: Some(collection_metadata),
            collection_master_edition: Some(collection_master_edition),
            system_program,
            sysvar_instructions,
        },
    );
    verify_collection.invoke_signed(signer_seeds)?;

    msg!("Collection Verified!");

    Ok(())
}
