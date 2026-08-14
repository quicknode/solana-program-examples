use anchor_lang::prelude::*;

/// Basis-point denominator: 100% = 10_000 bps. The spread and the oracle
/// confidence limit are expressed in basis points and divided by this.
#[constant]
pub const BASIS_POINTS_DENOMINATOR: u64 = 10_000;

/// Reject an oracle price older than this many slots. Slot count is what the
/// runtime guarantees; unix timestamps are validator-influenced. How long the
/// window is in seconds follows the cluster's slot time, which the protocol
/// lowers over time, so the window tightens on its own and never loosens. For a
/// market maker this bound is not a nicety: a stale quote is a free option for
/// whoever notices first.
pub const MAX_PRICE_STALENESS_SLOTS: u64 = 150;

#[constant]
pub const MARKET_SEED: &[u8] = b"market";

#[constant]
pub const AUTHORITY_SEED: &[u8] = b"authority";

#[constant]
pub const BASE_VAULT_SEED: &[u8] = b"base_vault";

#[constant]
pub const QUOTE_VAULT_SEED: &[u8] = b"quote_vault";
