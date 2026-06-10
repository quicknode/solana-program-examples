#![cfg_attr(not(test), no_std)]

use quasar_lang::prelude::*;
use quasar_spl::prelude::*;

#[cfg(test)]
mod tests;

declare_id!("FYPkt5VWMvtyWZDMGCwoKFkE3wXTzphicTpnNGuHWVbD");

/// User account storing the Solana authority and linked Ethereum address.
#[account(discriminator = 1, set_inner)]
pub struct UserAccount {
    pub authority: Address,
    pub ethereum_address: [u8; 20],
    /// Strictly increasing counter committed into every signed transfer
    /// authorization, so each Ethereum signature executes exactly once.
    pub nonce: u64,
}

/// Marker carrying the seeds for the per-user PDA: just the user account
/// address (no string prefix). Referenced through
/// `address = UserPda::seeds(...)` in the account constraints.
#[derive(Seeds)]
#[seeds(b"", user_account: Address)]
pub struct UserPda;

#[error_code]
pub enum ExternalDelegateError {
    /// Matches the Anchor variant's error codes, which start at 6000.
    InvalidSignature = 6000,
    NonceOverflow,
}

/// External delegate token master: allows transfers authorised either by
/// the Solana authority or by an Ethereum signature (secp256k1).
#[program]
mod quasar_external_delegate_token_master {
    use super::*;

    /// Initialize a user account with zero Ethereum address.
    #[instruction(discriminator = 0)]
    pub fn initialize(ctx: Ctx<InitializeAccountConstraints>) -> Result<(), ProgramError> {
        handle_initialize(&mut ctx.accounts)
    }

    /// Set the Ethereum address for signature verification.
    #[instruction(discriminator = 1)]
    pub fn set_ethereum_address(
        ctx: Ctx<SetEthereumAddressAccountConstraints>,
        ethereum_address: [u8; 20],
    ) -> Result<(), ProgramError> {
        handle_set_ethereum_address(&mut ctx.accounts, ethereum_address)
    }

    /// Transfer tokens using an Ethereum signature for authorisation.
    #[instruction(discriminator = 2)]
    pub fn transfer_tokens(
        ctx: Ctx<TransferTokensAccountConstraints>,
        amount: u64,
        signature: [u8; 65],
    ) -> Result<(), ProgramError> {
        handle_transfer_tokens(&mut ctx.accounts, amount, &signature, &ctx.bumps)
    }

    /// Transfer tokens using the Solana authority directly.
    #[instruction(discriminator = 3)]
    pub fn authority_transfer(
        ctx: Ctx<AuthorityTransferAccountConstraints>,
        amount: u64,
    ) -> Result<(), ProgramError> {
        handle_authority_transfer(&mut ctx.accounts, amount, &ctx.bumps)
    }
}

// ---------------------------------------------------------------------------
// Instruction accounts
// ---------------------------------------------------------------------------

#[derive(Accounts)]
pub struct InitializeAccountConstraints {
    #[account(mut, init, payer = authority)]
    pub user_account: Account<UserAccount>,
    #[account(mut)]
    pub authority: Signer,
    pub system_program: Program<SystemProgram>,
}

#[inline(always)]
fn handle_initialize(accounts: &mut InitializeAccountConstraints) -> Result<(), ProgramError> {
    accounts.user_account.set_inner(UserAccountInner {
        authority: *accounts.authority.address(),
        ethereum_address: [0u8; 20],
        nonce: 0,
    });
    Ok(())
}

#[derive(Accounts)]
pub struct SetEthereumAddressAccountConstraints {
    #[account(mut)]
    pub user_account: Account<UserAccount>,
    pub authority: Signer,
}

#[inline(always)]
fn handle_set_ethereum_address(
    accounts: &mut SetEthereumAddressAccountConstraints,
    ethereum_address: [u8; 20],
) -> Result<(), ProgramError> {
    require_keys_eq!(
        accounts.user_account.authority,
        *accounts.authority.address(),
        ProgramError::MissingRequiredSignature
    );
    accounts.user_account.ethereum_address = ethereum_address;
    Ok(())
}

#[derive(Accounts)]
pub struct TransferTokensAccountConstraints {
    #[account(mut)]
    pub user_account: Account<UserAccount>,
    pub authority: Signer,
    pub mint: Account<Mint>,
    #[account(mut)]
    pub user_token_account: Account<Token>,
    #[account(mut)]
    pub recipient_token_account: Account<Token>,
    /// PDA derived from user_account address.
    #[account(address = UserPda::seeds(user_account.address()))]
    pub user_pda: UncheckedAccount,
    pub token_program: Program<TokenProgram>,
}

#[inline(always)]
fn handle_transfer_tokens(
    accounts: &mut TransferTokensAccountConstraints,
    amount: u64,
    signature: &[u8; 65],
    bumps: &TransferTokensAccountConstraintsBumps,
) -> Result<(), ProgramError> {
    // The Ethereum signature supplements the Solana-side authority check;
    // it does not replace it.
    require_keys_eq!(
        accounts.user_account.authority,
        *accounts.authority.address(),
        ProgramError::MissingRequiredSignature
    );

    // Rebuild the authorized message onchain so the signature commits to
    // this exact transfer (amount, recipient, and the current nonce).
    let nonce: u64 = accounts.user_account.nonce.into();
    let message = build_transfer_authorization_message(
        accounts.user_account.address(),
        amount,
        accounts.recipient_token_account.address(),
        nonce,
    );

    if !verify_ethereum_signature(&accounts.user_account.ethereum_address, &message, signature) {
        return Err(ExternalDelegateError::InvalidSignature.into());
    }

    // Consume the nonce before the transfer CPI (checks-effects-interactions),
    // so this signature can never authorize a second execution.
    let next_nonce = nonce
        .checked_add(1)
        .ok_or(ExternalDelegateError::NonceOverflow)?;
    accounts.user_account.nonce = PodU64::from(next_nonce);

    let bump = [bumps.user_pda];
    let seeds: &[Seed] = &[
        Seed::from(accounts.user_account.address().as_ref()),
        Seed::from(&bump as &[u8]),
    ];

    accounts
        .token_program
        .transfer_checked(
            &accounts.user_token_account,
            &accounts.mint,
            &accounts.recipient_token_account,
            &accounts.user_pda,
            amount,
            accounts.mint.decimals,
        )
        .invoke_signed(seeds)
}

#[derive(Accounts)]
pub struct AuthorityTransferAccountConstraints {
    pub user_account: Account<UserAccount>,
    pub authority: Signer,
    pub mint: Account<Mint>,
    #[account(mut)]
    pub user_token_account: Account<Token>,
    #[account(mut)]
    pub recipient_token_account: Account<Token>,
    /// PDA derived from user_account address.
    #[account(address = UserPda::seeds(user_account.address()))]
    pub user_pda: UncheckedAccount,
    pub token_program: Program<TokenProgram>,
}

#[inline(always)]
fn handle_authority_transfer(
    accounts: &mut AuthorityTransferAccountConstraints,
    amount: u64,
    bumps: &AuthorityTransferAccountConstraintsBumps,
) -> Result<(), ProgramError> {
    require_keys_eq!(
        accounts.user_account.authority,
        *accounts.authority.address(),
        ProgramError::MissingRequiredSignature
    );

    let bump = [bumps.user_pda];
    let seeds: &[Seed] = &[
        Seed::from(accounts.user_account.address().as_ref()),
        Seed::from(&bump as &[u8]),
    ];

    accounts
        .token_program
        .transfer_checked(
            &accounts.user_token_account,
            &accounts.mint,
            &accounts.recipient_token_account,
            &accounts.user_pda,
            amount,
            accounts.mint.decimals,
        )
        .invoke_signed(seeds)
}

// ---------------------------------------------------------------------------
// Transfer authorization message
// ---------------------------------------------------------------------------

/// Byte length of the transfer authorization preimage: program id, user
/// account, amount, recipient token account, nonce.
const TRANSFER_AUTHORIZATION_PREIMAGE_LEN: usize =
    core::mem::size_of::<Address>() * 3 + core::mem::size_of::<u64>() * 2;

/// Reconstructs the message a delegate must sign to authorize one transfer:
/// keccak256(program id || user account || amount LE || recipient token account || nonce LE).
///
/// Because the hash commits to every transfer parameter plus the user
/// account's stored nonce, a signature is valid for exactly one
/// (amount, recipient, nonce) execution and cannot be replayed.
fn build_transfer_authorization_message(
    user_account: &Address,
    amount: u64,
    recipient_token_account: &Address,
    nonce: u64,
) -> [u8; 32] {
    let amount_bytes = amount.to_le_bytes();
    let nonce_bytes = nonce.to_le_bytes();
    let parts: [&[u8]; 5] = [
        ID.as_ref(),
        user_account.as_ref(),
        &amount_bytes,
        recipient_token_account.as_ref(),
        &nonce_bytes,
    ];

    let mut preimage = [0u8; TRANSFER_AUTHORIZATION_PREIMAGE_LEN];
    let mut offset = 0usize;
    for part in parts {
        preimage[offset..offset + part.len()].copy_from_slice(part);
        offset += part.len();
    }
    keccak256(&preimage)
}

// ---------------------------------------------------------------------------
// Ethereum signature verification using raw syscalls
// ---------------------------------------------------------------------------

fn keccak256(data: &[u8]) -> [u8; 32] {
    let hash = solana_keccak_hasher::hash(data);
    let bytes: &[u8] = hash.as_ref();
    let mut result = [0u8; 32];
    result.copy_from_slice(bytes);
    result
}

/// Recover secp256k1 public key from a signature, using the raw Solana syscall.
///
/// Returns `None` if recovery fails. The returned key is the 65-byte
/// uncompressed public key (first byte `0x04` is omitted by the syscall,
/// only the 64 bytes of x||y are returned).
fn secp256k1_recover(
    message_hash: &[u8; 32],
    recovery_id: u8,
    signature: &[u8; 64],
) -> Option<[u8; 64]> {
    #[cfg(target_os = "solana")]
    {
        let mut pubkey_result = [0u8; 64];
        let rc = unsafe {
            solana_define_syscall::definitions::sol_secp256k1_recover(
                message_hash.as_ptr(),
                recovery_id as u64,
                signature.as_ptr(),
                pubkey_result.as_mut_ptr(),
            )
        };
        if rc == 0 {
            Some(pubkey_result)
        } else {
            None
        }
    }
    #[cfg(not(target_os = "solana"))]
    {
        // Offchain: not implemented (would need a secp256k1 library).
        let _ = (message_hash, recovery_id, signature);
        None
    }
}

fn verify_ethereum_signature(
    ethereum_address: &[u8; 20],
    message: &[u8; 32],
    signature: &[u8; 65],
) -> bool {
    let recovery_id = signature[64];
    let mut sig = [0u8; 64];
    sig.copy_from_slice(&signature[..64]);

    if let Some(pubkey_bytes) = secp256k1_recover(message, recovery_id, &sig) {
        // Ethereum address = last 20 bytes of keccak256(public_key)
        // The syscall returns the 64-byte uncompressed key (sans prefix byte).
        let hash = keccak256(&pubkey_bytes);
        let mut recovered_address = [0u8; 20];
        recovered_address.copy_from_slice(&hash[12..]);
        recovered_address == *ethereum_address
    } else {
        false
    }
}
