use anchor_lang::prelude::*;
use sha3::{Digest, Keccak256};
use solana_secp256k1_recover::secp256k1_recover;

mod instructions;
use instructions::*;

declare_id!("FYPkt5VWMvtyWZDMGCwoKFkE3wXTzphicTpnNGuHWVbD");

#[program]
pub mod external_delegate_token_master {
    use super::*;

    pub fn initialize(context: &mut Context<InitializeAccountConstraints>) -> Result<()> {
        instructions::initialize::handler(context)
    }

    pub fn set_ethereum_address(
        context: &mut Context<SetEthereumAddressAccountConstraints>,
        ethereum_address: [u8; 20],
    ) -> Result<()> {
        instructions::set_ethereum_address::handler(context, ethereum_address)
    }

    pub fn transfer_tokens(
        context: &mut Context<TransferTokensAccountConstraints>,
        amount: u64,
        signature: [u8; 65],
    ) -> Result<()> {
        instructions::transfer_tokens::handler(context, amount, signature)
    }

    pub fn authority_transfer(
        context: &mut Context<AuthorityTransferAccountConstraints>,
        amount: u64,
    ) -> Result<()> {
        instructions::authority_transfer::handler(context, amount)
    }
}

#[account(borsh)]
#[derive(InitSpace)]
pub struct UserAccount {
    pub authority: Address,
    pub ethereum_address: [u8; 20],
    /// Strictly increasing counter committed into every signed transfer
    /// authorization, so each Ethereum signature executes exactly once.
    pub nonce: u64,
}

#[error_code]
pub enum ErrorCode {
    #[msg("Invalid Ethereum signature")]
    InvalidSignature,
    #[msg("Nonce overflow")]
    NonceOverflow,
}

/// Reconstructs the message a delegate must sign to authorize one transfer:
/// keccak256(program id || user account || amount LE || recipient token account || nonce LE).
///
/// Because the hash commits to every transfer parameter plus the user
/// account's stored nonce, a signature is valid for exactly one
/// (amount, recipient, nonce) execution and cannot be replayed.
pub fn build_transfer_authorization_message(
    user_account: &Address,
    amount: u64,
    recipient_token_account: &Address,
    nonce: u64,
) -> [u8; 32] {
    let mut hasher = Keccak256::new();
    hasher.update(ID.as_ref());
    hasher.update(user_account.as_ref());
    hasher.update(amount.to_le_bytes());
    hasher.update(recipient_token_account.as_ref());
    hasher.update(nonce.to_le_bytes());
    hasher.finalize().into()
}

pub fn verify_ethereum_signature(
    ethereum_address: &[u8; 20],
    message: &[u8; 32],
    signature: &[u8; 65],
) -> bool {
    let recovery_id = signature[64];
    let mut sig = [0u8; 64];
    sig.copy_from_slice(&signature[..64]);

    if let Ok(pubkey) = secp256k1_recover(message, recovery_id, &sig) {
        // An Ethereum address is the last 20 bytes of the keccak256 hash of
        // the 64-byte uncompressed public key (x || y, no 0x04 prefix byte).
        // `secp256k1_recover` already returns exactly those 64 bytes.
        let pubkey_bytes = pubkey.to_bytes();
        let mut recovered_address = [0u8; 20];
        recovered_address.copy_from_slice(&keccak256(&pubkey_bytes)[12..]);
        recovered_address == *ethereum_address
    } else {
        false
    }
}

fn keccak256(data: &[u8]) -> [u8; 32] {
    let mut hasher = Keccak256::new();
    hasher.update(data);
    hasher.finalize().into()
}
