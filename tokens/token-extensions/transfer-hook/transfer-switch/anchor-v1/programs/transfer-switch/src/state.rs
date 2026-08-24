use anchor_lang::prelude::*;

#[account]
#[derive(InitSpace)]
pub struct TransferSwitch {
    pub wallet: Pubkey,
    pub on: bool,
    /// Canonical bump for this PDA.
    pub bump: u8,
}

#[account]
#[derive(InitSpace)]
pub struct AdminConfig {
    pub is_initialised: bool,
    pub admin: Pubkey,
    /// Canonical bump for this PDA.
    pub bump: u8,
}
