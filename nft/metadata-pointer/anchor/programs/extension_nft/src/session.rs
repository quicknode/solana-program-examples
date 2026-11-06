//! A local reader for gpl-session's `SessionToken` account.
//!
//! The `session-keys` crate is Anchor v1 only: its `Session` derive requires a
//! field of type `Option<Account<'info, SessionToken>>`, and `SessionToken` is
//! not `Pod`, so v2's zero-copy `Account<T>` cannot hold it either. The account
//! itself is just four `Address`es worth of borsh data owned by the session-keys
//! program, so this module declares that layout, checks the discriminator and
//! PDA, and leaves the gasless-session lesson intact without the dependency.
//!
//! WARNING, unchanged from before: the session-keys program has not been
//! audited. A session token lets a player sign game transactions with a
//! short-lived key instead of their main wallet. Do not ship this pattern
//! without reviewing the session-keys source and hardening issuance.

use anchor_lang::prelude::*;

/// `KeyspM2ssCJbqUhQ4k7sveSiY4WjnYsrXkC8oDbwde5`, the session-keys program.
pub const SESSION_KEYS_ID: Address =
    anchor_lang::address!("KeyspM2ssCJbqUhQ4k7sveSiY4WjnYsrXkC8oDbwde5");

/// First seed of the session-token PDA.
pub const SESSION_TOKEN_SEED: &[u8] = b"session_token";

/// gpl-session's `SessionToken`, as it is laid out on chain. Anchor's borsh
/// account encoding puts the eight-byte discriminator first, then the fields in
/// declaration order.
#[account(borsh)]
pub struct SessionToken {
    pub authority: Address,
    pub target_program: Address,
    pub session_signer: Address,
    pub valid_until: i64,
}

/// Whether `session_token` is a live session for `authority`, signed by
/// `session_signer`, targeting this program.
///
/// Returns `false` rather than erroring when the account is absent, malformed,
/// or expired, so the caller can fall back to plain wallet authorization.
pub fn is_valid_session(
    session_token: &UncheckedAccount,
    session_signer: &Address,
    authority: &Address,
) -> Result<bool> {
    if !session_token.account().owned_by(&SESSION_KEYS_ID) {
        return Ok(false);
    }

    let data = session_token.account().try_borrow()?;
    let disc_len = <SessionToken as anchor_lang::Discriminator>::DISCRIMINATOR.len();
    if data.len() <= disc_len
        || &data[..disc_len] != <SessionToken as anchor_lang::Discriminator>::DISCRIMINATOR
    {
        return Ok(false);
    }
    let mut payload = &data[disc_len..];
    let Ok(token) =
        <SessionToken as wincode::SchemaRead<anchor_lang::BorshConfig>>::get(&mut payload)
    else {
        return Ok(false);
    };

    // The PDA binds the token to exactly one (target_program, signer, authority)
    // triple, so deriving it is what proves the token is the caller's.
    let (expected, _bump) = Pubkey::find_program_address(
        &[
            SESSION_TOKEN_SEED,
            crate::ID.as_ref(),
            session_signer.as_ref(),
            authority.as_ref(),
        ],
        &SESSION_KEYS_ID,
    );
    if expected != *session_token.address() {
        return Ok(false);
    }

    Ok(Clock::get()?.unix_timestamp < token.valid_until)
}
