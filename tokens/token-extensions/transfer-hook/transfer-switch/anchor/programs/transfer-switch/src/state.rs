use anchor_lang::prelude::*;

#[account(borsh)]
#[derive(InitSpace)]
pub struct TransferSwitch {
    pub wallet: Address,
    pub on: bool,
    /// Canonical bump for this PDA.
    pub bump: u8,
}

#[account(borsh)]
#[derive(InitSpace)]
pub struct AdminConfig {
    pub is_initialised: bool,
    pub admin: Address,
    /// Canonical bump for this PDA.
    pub bump: u8,
}
