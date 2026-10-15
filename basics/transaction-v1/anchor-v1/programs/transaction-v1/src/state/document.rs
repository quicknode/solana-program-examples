use anchor_lang::prelude::*;

// There is deliberately no `InitSpace` here: the account is sized to each
// document rather than to a compile-time maximum.
#[account]
pub struct Document {
    pub data: Vec<u8>,
}

impl Document {
    pub const SEED_PREFIX: &'static [u8; 8] = b"document";

    /// Discriminator, then the borsh length prefix, then the bytes.
    pub fn required_space(document_len: usize) -> usize {
        Self::DISCRIMINATOR.len() + 4 + document_len
    }
}
