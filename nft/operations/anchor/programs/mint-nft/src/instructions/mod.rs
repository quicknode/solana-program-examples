pub mod create_collection;
pub mod mint_nft;
pub mod verify_collection;

pub use create_collection::*;
pub use mint_nft::*;
pub use verify_collection::*;

use {
    crate::error::MintNftError,
    anchor_lang::prelude::*,
    anchor_spl::metadata::mpl_token_metadata::{
        MAX_NAME_LENGTH, MAX_SYMBOL_LENGTH, MAX_URI_LENGTH,
    },
};

/// Rejects metadata strings that exceed the Metaplex Token Metadata limits,
/// so callers get a named error instead of an opaque CPI failure.
pub fn validate_metadata_strings(name: &str, symbol: &str, uri: &str) -> Result<()> {
    require!(name.len() <= MAX_NAME_LENGTH, MintNftError::NameTooLong);
    require!(
        symbol.len() <= MAX_SYMBOL_LENGTH,
        MintNftError::SymbolTooLong
    );
    require!(uri.len() <= MAX_URI_LENGTH, MintNftError::UriTooLong);
    Ok(())
}
