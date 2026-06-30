//! Shared constants. See the Anchor sibling for the prose explanations; the
//! values are identical so the two implementations behave the same.

/// 100% expressed in basis points.
pub const BASIS_POINTS_DENOMINATOR: u64 = 10_000;

/// Fixed-point precision for the cumulative funding index.
pub const FUNDING_PRECISION: i128 = 1_000_000_000;

/// Fixed-point precision for the per-side `size / entry_price` accumulators.
pub const SIZE_PRECISION: u128 = 1_000_000_000;

/// Liquidity-provider shares withheld from the first deposit so the share
/// supply never starts at a dust amount.
pub const MINIMUM_LIQUIDITY: u64 = 1_000;

/// Reject an oracle price older than this many slots (~1 minute at 400ms).
pub const MAX_PRICE_STALENESS_SLOTS: u64 = 150;

/// Upper bound on a pool's configurable `max_leverage`.
pub const MAX_LEVERAGE_CEILING: u16 = 100;

/// Long / short discriminants, used both as the position-PDA seed byte and the
/// `side` instruction argument.
pub const SIDE_LONG: u8 = 0;
pub const SIDE_SHORT: u8 = 1;
