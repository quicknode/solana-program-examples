use anchor_lang::prelude::*;

/// Fixed-point scale for every ratio in the program: interest rates, the
/// cumulative borrow-rate index, the share-token exchange rate, and obligation
/// values. A ratio `r` is stored as the integer `r * FIXED_POINT_SCALE`.
///
/// All money math is integer-only (no floats, no fixed-point crates). 10^18
/// keeps a single slot's interest — which can be a tiny fraction of the index —
/// from truncating to zero, while u128's ~3.4e38 ceiling leaves headroom for the
/// index to grow and for intermediate products before the final narrowing cast.
#[constant]
pub const FIXED_POINT_SCALE: u128 = 1_000_000_000_000_000_000;

/// log10(FIXED_POINT_SCALE). Used to fold the price exponent and the fixed-point
/// scale into one power of ten so price conversions never form a needless 10^18
/// intermediate that would overflow for high-priced assets.
pub const FIXED_POINT_SCALE_DECIMALS: i32 = 18;

/// Denominator for every basis-point config value. 100% == 10_000 bps.
#[constant]
pub const BPS_DENOMINATOR: u128 = 10_000;

/// Slots per year, for turning an APR (in bps) into a per-slot rate.
/// Solana targets ~2.5 slots/second: 2.5 * 60 * 60 * 24 * 365 = 78_840_000.
#[constant]
pub const SLOTS_PER_YEAR: u128 = 78_840_000;

/// Maximum distinct reserves an obligation may use as collateral, and
/// separately as borrows. Bounds the account size and the compute cost of
/// refresh_obligation (which iterates every entry).
pub const MAX_OBLIGATION_RESERVES: usize = 4;

/// A price feed older than this many slots is rejected as stale (~10s at 2.5
/// slots/second). Freshness is measured in slots, not unix time, because the
/// runtime guarantees slot progression while the timestamp is validator-influenced.
#[constant]
pub const MAX_PRICE_STALENESS_SLOTS: u64 = 25;

// PDA seeds.
pub const LENDING_MARKET_SEED: &[u8] = b"lending_market";
pub const RESERVE_SEED: &[u8] = b"reserve";
pub const LIQUIDITY_VAULT_SEED: &[u8] = b"liquidity_vault";
pub const SHARE_MINT_SEED: &[u8] = b"share_mint";
pub const OBLIGATION_SEED: &[u8] = b"obligation";
pub const OBLIGATION_SHARE_VAULT_SEED: &[u8] = b"obligation_share_vault";
pub const PRICE_FEED_SEED: &[u8] = b"price_feed";
