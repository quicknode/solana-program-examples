use anchor_lang::prelude::*;

#[account(borsh)]
#[derive(InitSpace)]
pub struct Contribution {
    pub amount: u64,
    /// Canonical bump for this PDA.
    pub bump: u8,
}
