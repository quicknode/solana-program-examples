use anchor_lang::prelude::*;

/// Basis-point denominator: 100% = 10_000 bps. The venue's fee on each premium
/// is expressed in basis points and divided by this.
#[constant]
pub const BASIS_POINTS_DENOMINATOR: u64 = 10_000;

#[constant]
pub const MARKET_SEED: &[u8] = b"market";

#[constant]
pub const AUTHORITY_SEED: &[u8] = b"authority";

#[constant]
pub const UNDERLYING_VAULT_SEED: &[u8] = b"underlying_vault";

#[constant]
pub const QUOTE_VAULT_SEED: &[u8] = b"quote_vault";

#[constant]
pub const OPTION_SEED: &[u8] = b"option";
