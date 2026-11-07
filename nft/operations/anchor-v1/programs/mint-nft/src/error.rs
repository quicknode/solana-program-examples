use anchor_lang::prelude::*;

#[error_code]
pub enum MintNftError {
    #[msg("Metadata name exceeds the Metaplex maximum of 32 bytes")]
    NameTooLong,
    #[msg("Metadata symbol exceeds the Metaplex maximum of 10 bytes")]
    SymbolTooLong,
    #[msg("Metadata URI exceeds the Metaplex maximum of 200 bytes")]
    UriTooLong,
}
