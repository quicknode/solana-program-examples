#![cfg_attr(not(test), no_std)]

use quasar_lang::prelude::*;

pub mod instructions;
use instructions::*;
#[cfg(test)]
mod tests;

declare_id!("3EMcczaGi9ivdLxvvFwRbGYeEUEHpGwabXegARw4jLxa");

/// Marker carrying the seeds for the shared PDA mint authority used as
/// both mint and update authority. Quasar derives PDA addresses through a
/// `#[derive(Seeds)]` type referenced by `address = T::seeds()`.
#[derive(Seeds)]
#[seeds(b"authority")]
pub struct MintAuthorityPda;

/// NFT operations: create a collection, mint NFTs into it, and verify
/// collection membership.
///
/// Uses a PDA (`["authority"]`) as the mint authority and update authority
/// for both the collection and individual NFTs.
#[program]
mod quasar_nft_operations {
    use super::*;

    // String capacities follow the Metaplex Token Metadata limits:
    // name <= 32, symbol <= 10, uri <= 200 bytes. The bounded types reject
    // oversized values at instruction decoding.

    /// Create a collection NFT: mint, metadata, and master edition.
    #[instruction(discriminator = 0)]
    pub fn create_collection(
        ctx: Ctx<CreateCollectionAccountConstraints>,
        name: String<32>,
        symbol: String<10>,
        uri: String<200>,
    ) -> Result<(), ProgramError> {
        instructions::handle_create_collection(&mut ctx.accounts, &ctx.bumps, name, symbol, uri)
    }

    /// Mint an individual NFT with an unverified reference to the collection.
    #[instruction(discriminator = 1)]
    pub fn mint_nft(
        ctx: Ctx<MintNftAccountConstraints>,
        name: String<32>,
        symbol: String<10>,
        uri: String<200>,
    ) -> Result<(), ProgramError> {
        instructions::handle_mint_nft(&mut ctx.accounts, &ctx.bumps, name, symbol, uri)
    }

    /// Verify the NFT as a member of the collection.
    #[instruction(discriminator = 2)]
    pub fn verify_collection(
        ctx: Ctx<VerifyCollectionMintAccountConstraints>,
    ) -> Result<(), ProgramError> {
        instructions::handle_verify_collection(&mut ctx.accounts, &ctx.bumps)
    }
}
