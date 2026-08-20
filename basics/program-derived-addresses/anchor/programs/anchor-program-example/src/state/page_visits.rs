use anchor_lang::prelude::*;

#[account]
#[derive(InitSpace)] // automatically calculate the space required for the struct
pub struct PageVisits {
    pub page_visits: u32,
    pub bump: u8,
    // v2's `#[account]` is zero-copy, so the struct has to be Pod, and Pod
    // rejects implicit padding. u32 + u8 leaves three bytes, so name them.
    pub _padding: [u8; 3],
}

impl PageVisits {
    pub const SEED_PREFIX: &'static [u8; 11] = b"page_visits";
}
