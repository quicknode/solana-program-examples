//! PDA derivation and verification helpers for Metaplex Token Metadata.
//!
//! Canonical seeds:
//! - Metadata: `["metadata", metadata_program_id, mint]`
//! - Master Edition: `["metadata", metadata_program_id, mint, "edition"]`

use {
    crate::constants::{METADATA_PROGRAM_BYTES, METADATA_PROGRAM_ID},
    quasar_lang::__solana_program_error::ProgramError,
    solana_address::Address,
};

const METADATA_SEED: &[u8] = b"metadata";
const EDITION_SEED: &[u8] = b"edition";

/// Derive the metadata PDA address from a mint.
///
/// **Cost:** ~544 CU (SHA-256 loop). Prefer [`verify_metadata_address`] when
/// the address is already known.
#[inline(always)]
pub fn metadata_address(mint: &Address) -> (Address, u8) {
    quasar_lang::pda::try_find_program_address(
        &[METADATA_SEED, &METADATA_PROGRAM_BYTES, mint.as_ref()],
        &METADATA_PROGRAM_ID,
    )
    .unwrap()
}

/// Derive the master edition PDA address from a mint.
///
/// **Cost:** ~544 CU. Prefer [`verify_master_edition_address`] when known.
#[inline(always)]
pub fn master_edition_address(mint: &Address) -> (Address, u8) {
    quasar_lang::pda::try_find_program_address(
        &[
            METADATA_SEED,
            &METADATA_PROGRAM_BYTES,
            mint.as_ref(),
            EDITION_SEED,
        ],
        &METADATA_PROGRAM_ID,
    )
    .unwrap()
}

/// Verify a metadata address matches the expected PDA for a mint.
///
/// **Cost:** ~90 CU per attempt (single SHA-256 + compare).
#[inline(always)]
pub fn verify_metadata_address(address: &Address, mint: &Address) -> Result<(), ProgramError> {
    quasar_lang::pda::find_bump_for_address(
        &[METADATA_SEED, &METADATA_PROGRAM_BYTES, mint.as_ref()],
        &METADATA_PROGRAM_ID,
        address,
    )
    .map(|_| ())
    .map_err(|_| ProgramError::InvalidSeeds)
}

/// Verify a master edition address matches the expected PDA for a mint.
///
/// **Cost:** ~90 CU per attempt.
#[inline(always)]
pub fn verify_master_edition_address(
    address: &Address,
    mint: &Address,
) -> Result<(), ProgramError> {
    quasar_lang::pda::find_bump_for_address(
        &[
            METADATA_SEED,
            &METADATA_PROGRAM_BYTES,
            mint.as_ref(),
            EDITION_SEED,
        ],
        &METADATA_PROGRAM_ID,
        address,
    )
    .map(|_| ())
    .map_err(|_| ProgramError::InvalidSeeds)
}
