use anchor_lang::prelude::*;

declare_id!("3EMcczaGi9ivdLxvvFwRbGYeEUEHpGwabXegARw4jLxa");

pub mod error;
pub mod instructions;

pub use instructions::*;

#[program]
pub mod mint_nft {

    use super::*;

    /// Create a collection NFT with the given metadata.
    pub fn create_collection(
        mut context: &mut Context<CreateCollectionAccountConstraints>,
        name: String,
        symbol: String,
        uri: String,
    ) -> Result<()> {
        instructions::create_collection::handle_create_collection(
            &mut context.accounts,
            &context.bumps,
            name,
            symbol,
            uri,
        )
    }

    /// Mint an NFT into the collection with the given metadata.
    pub fn mint_nft(
        mut context: &mut Context<MintNftAccountConstraints>,
        name: String,
        symbol: String,
        uri: String,
    ) -> Result<()> {
        instructions::mint_nft::handle_mint_nft(&mut context.accounts, &context.bumps, name, symbol, uri)
    }

    /// Verify an NFT as a member of the collection.
    pub fn verify_collection(
        mut context: &mut Context<VerifyCollectionMintAccountConstraints>,
    ) -> Result<()> {
        instructions::verify_collection::handle_verify_collection(
            &mut context.accounts,
            &context.bumps,
        )
    }
}
