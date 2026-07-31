//! Shared constants for the Quasar lending program.

/// Fixed-point scale (10^18) for every ratio: interest rates, the cumulative
/// borrow-rate index, the share-token exchange rate, and obligation values.
/// All money math is integer-only `u128`; a ratio `r` is stored as
/// `r * FIXED_POINT_SCALE`.
pub const FIXED_POINT_SCALE: u128 = 1_000_000_000_000_000_000;

/// log10(FIXED_POINT_SCALE). Folds the price exponent and the fixed-point scale
/// into one power of ten so price conversions never form a needless 10^18
/// intermediate that would overflow for high-priced assets.
pub const FIXED_POINT_SCALE_DECIMALS: i32 = 18;

/// 100% expressed in basis points.
pub const BPS_DENOMINATOR: u128 = 10_000;

/// Slots per year (~2.5 slots/s), for turning an APR in bps into a per-slot rate.
pub const SLOTS_PER_YEAR: u128 = 78_840_000;

/// Reject a price feed older than this many slots (~10s at 2.5 slots/s).
pub const MAX_PRICE_STALENESS_SLOTS: u64 = 25;

/// SPL token account size, for the rent-exempt vault created in `initialize_reserve`.
pub const TOKEN_ACCOUNT_SPACE: u64 = 165;

/// SPL mint size, for the rent-exempt share mint created in `initialize_reserve`.
pub const MINT_SPACE: u64 = 82;

// PDA seeds for the `Seed::from(...)` signer arrays in the CPI-signing handlers.
// (The `#[seeds(...)]` attributes on the account types carry their own literals.)
pub const RESERVE_SEED: &[u8] = b"reserve";
pub const LIQUIDITY_VAULT_SEED: &[u8] = b"liquidity_vault";
pub const SHARE_MINT_SEED: &[u8] = b"share_mint";
pub const OBLIGATION_SEED: &[u8] = b"obligation";
