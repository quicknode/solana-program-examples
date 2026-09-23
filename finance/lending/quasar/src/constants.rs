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

/// Seconds in a 365-day year: the divisor that turns an annual rate into the
/// per-second rate interest accrues at. Interest runs on the wall clock, not
/// the slot count, because a rate quoted per year is a promise about wall-clock
/// time and a slots-per-year divisor is only a guess at the slot length.
pub const SECONDS_PER_YEAR: u128 = 31_536_000;

/// Reject a price feed older than this many slots. Freshness is counted in
/// slots, not unix time, because the runtime guarantees slot progression while
/// the timestamp is validator-influenced. How long the window is in seconds
/// follows the cluster's slot time, which the protocol lowers over time, so the
/// window tightens on its own and never loosens.
pub const MAX_PRICE_STALENESS_SLOTS: u64 = 25;

/// SPL token account size, for the rent-exempt vault created in `initialize_reserve`.
pub const TOKEN_ACCOUNT_SPACE: u64 = 165;

/// SPL mint size, for the rent-exempt share mint created in `initialize_reserve`.
pub const MINT_SPACE: u64 = 82;

/// Reserve shares withheld from a reserve's first deposit. The first supplier
/// receives `deposit - MINIMUM_SHARES` shares rather than the full amount, the
/// convention Uniswap V2 uses for LP tokens, and every conversion between
/// shares and liquidity counts these as shares that nobody holds
/// (`math::total_shares`), so their slice of the pool never leaves.
///
/// Shares are priced off tracked `total_liquidity`, not the vault balance, so a
/// direct donation cannot move them. But `total_liquidity` counts interest owed
/// on borrows, and a supplier can also borrow: a lone supplier holding one
/// share can make it worth more than one unit through their own debt, then
/// ratchet the price up with deposits and redemptions whose rounding stays in
/// the pool, until a later deposit rounds down and they take part of it. With
/// the minimum counted, their one share is 1 of 1_001, and whatever the
/// rounding leaves behind is spread mostly across shares they cannot redeem.
pub const MINIMUM_SHARES: u64 = 1_000;

// PDA seeds for the `Seed::from(...)` signer arrays in the CPI-signing handlers.
// (The `#[seeds(...)]` attributes on the account types carry their own literals.)
pub const RESERVE_SEED: &[u8] = b"reserve";
pub const LIQUIDITY_VAULT_SEED: &[u8] = b"liquidity_vault";
pub const SHARE_MINT_SEED: &[u8] = b"share_mint";
pub const OBLIGATION_SEED: &[u8] = b"obligation";
