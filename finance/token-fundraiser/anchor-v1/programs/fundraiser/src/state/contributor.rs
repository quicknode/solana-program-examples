use anchor_lang::prelude::*;

#[account]
#[derive(InitSpace)]
pub struct Contributor {
    pub amount: u64,
    /// Canonical bump for this PDA.
    pub bump: u8,
}