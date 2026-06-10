use anchor_lang::prelude::*;

/// Seed prefix for the vault PDA. The same PDA stores the withdraw authority
/// and acts as the cNFT leaf owner that signs Bubblegum transfers.
pub const VAULT_SEED: &[u8] = b"cNFT-vault";

#[derive(InitSpace)]
#[account]
pub struct Vault {
    /// The only signer allowed to withdraw cNFTs from the vault.
    pub authority: Pubkey,

    pub bump: u8,
}
