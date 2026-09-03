use anchor_lang::prelude::*;

// `borsh` because the struct holds a `Vec<u8>` whose length the caller
// chooses: v2's default `#[account]` backing is zero-copy and needs a `Pod`
// (fixed-layout) type. There is deliberately no `InitSpace` either: the
// account is sized to each document rather than to a compile-time maximum.
#[account(borsh)]
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
