// These are plain `pub const`s rather than Anchor `#[constant]`s: `#[constant]`
// only re-exports a value into the IDL, and anchor's idl-build mis-evaluates a
// u128 literal this large as i32 ("literal out of range for i32"). None of these
// need to appear in the IDL, so plain consts both compile and keep the IDL clean.

/// Fixed-point scale for every ratio in the program: interest rates, the
/// cumulative borrow-rate index, the share-token exchange rate, and obligation
/// values. A ratio `r` is stored as the integer `r * FIXED_POINT_SCALE`.
///
/// All money math is integer-only (no floats, no fixed-point crates). 10^18
/// keeps a single second's interest, which can be a tiny fraction of the index,
/// from truncating to zero, while u128's ~3.4e38 ceiling leaves headroom for the
/// index to grow and for intermediate products before the final narrowing cast.
pub const FIXED_POINT_SCALE: u128 = 1_000_000_000_000_000_000;

/// log10(FIXED_POINT_SCALE). Used to fold the price exponent and the fixed-point
/// scale into one power of ten so price conversions never form a needless 10^18
/// intermediate that would overflow for high-priced assets.
pub const FIXED_POINT_SCALE_DECIMALS: i32 = 18;

/// Denominator for every basis-point config value. 100% == 10_000 bps.
pub const BPS_DENOMINATOR: u128 = 10_000;

/// Maximum distinct reserves an obligation may use as collateral, and
/// separately as borrows. Bounds the account size and the compute cost of
/// refresh_obligation (which iterates every entry).
pub const MAX_OBLIGATION_RESERVES: usize = 4;

/// Seconds in a 365-day year: the divisor that turns an annual rate into the
/// per-second rate interest accrues at. Interest runs on the wall clock, not
/// the slot count, because a rate quoted per year is a promise about wall-clock
/// time and a slots-per-year divisor is only a guess at the slot length.
pub const SECONDS_PER_YEAR: u128 = 31_536_000;

/// A price feed older than this many slots is rejected as stale. Freshness is
/// measured in slots, not unix time, because the runtime guarantees slot
/// progression while the timestamp is validator-influenced. How long the window
/// is in seconds follows the cluster's slot time, which the protocol lowers over
/// time, so the window tightens on its own and never loosens.
pub const MAX_PRICE_STALENESS_SLOTS: u64 = 25;

/// Reserve shares withheld from a reserve's first deposit. The first supplier
/// receives `deposit - MINIMUM_SHARES` shares rather than the full amount, the
/// convention Uniswap V2 uses for LP tokens, and every conversion between
/// shares and liquidity counts these as shares that nobody holds
/// (`Reserve::total_shares`), so their slice of the pool never leaves.
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

// PDA seeds.
pub const LENDING_MARKET_SEED: &[u8] = b"lending_market";
pub const RESERVE_SEED: &[u8] = b"reserve";
pub const LIQUIDITY_VAULT_SEED: &[u8] = b"liquidity_vault";
pub const SHARE_MINT_SEED: &[u8] = b"share_mint";
pub const OBLIGATION_SEED: &[u8] = b"obligation";
pub const OBLIGATION_SHARE_VAULT_SEED: &[u8] = b"obligation_share_vault";
pub const PRICE_FEED_SEED: &[u8] = b"price_feed";
