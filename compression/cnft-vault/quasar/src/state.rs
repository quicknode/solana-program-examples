use quasar_lang::prelude::*;

/// Vault PDA state. The same PDA stores the withdraw authority, owns the
/// cNFTs (as Bubblegum leaf owner), and signs transfer CPIs via
/// invoke_signed.
#[account(discriminator = 1, set_inner)]
#[seeds(b"cNFT-vault")]
pub struct Vault {
    /// The only signer allowed to withdraw cNFTs from the vault.
    pub authority: Address,

    pub bump: u8,
}
