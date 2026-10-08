//! Shared constants. See the Anchor sibling for the prose explanations; the
//! values are identical so the two implementations behave the same.

/// 100% expressed in basis points.
pub const BASIS_POINTS_DENOMINATOR: u64 = 10_000;

/// Fixed-point precision for the cumulative funding index.
pub const FUNDING_PRECISION: i128 = 1_000_000_000;

/// Fixed-point precision for the per-side `size / entry_price` accumulators.
pub const SIZE_PRECISION: u128 = 1_000_000_000;

/// Fixed-point precision for the haircut ratio `h`, the fraction of their
/// profit every closing winner is paid. `HAIRCUT_PRECISION` is `h = 1` (profit
/// paid in full); a smaller value pays that fraction of it.
pub const HAIRCUT_PRECISION: u128 = 1_000_000_000;

/// Liquidity-provider shares withheld from the first deposit so the share
/// supply never starts at a dust amount. Both `add_liquidity` and
/// `remove_liquidity` divide by the share supply plus this minimum, so the
/// withheld shares belong to nobody and their slice of the pool never leaves.
pub const MINIMUM_LIQUIDITY: u64 = 1_000;

/// Reject an oracle price older than this many slots. Counted in slots because
/// the runtime guarantees slot progression; the seconds that comes to follow
/// the cluster's slot time, which the protocol lowers over time.
pub const MAX_PRICE_STALENESS_SLOTS: u64 = 150;

/// How many seconds of oracle prices the pool's `average_price` follows. Each
/// fold moves the average toward the price seen at the previous read by
/// `elapsed / window` of the gap between them, and an interval of a full window
/// or more replaces the average with that price. Ten minutes is long enough
/// that a price seen at two reads six seconds apart, about as long as a faulty
/// or manipulated oracle print lasts, moves the average by one percent of its
/// jump, and short enough that a genuine move is back inside the band within
/// minutes of repeated reads. Counted on the Clock's `unix_timestamp`,
/// like funding: it is a span of wall-clock time, and the second or two of
/// leader drift changes a fold's weight by well under one percent.
pub const PRICE_AVERAGE_WINDOW_SECONDS: i64 = 600;

/// Upper bound on a pool's `funding_rate_per_second`, in `FUNDING_PRECISION`
/// units: 277 billionths of a position's size per second, just under 0.1% of
/// its size per hour. The rate is fixed when the pool is created, so everyone
/// who opens a position or deposits liquidity has seen it, and no position can
/// be charged or paid funding faster than this.
pub const MAX_FUNDING_RATE_PER_SECOND: u64 = 277;

/// Long / short discriminants, used both as the position-PDA seed byte and the
/// `side` instruction argument.
pub const SIDE_LONG: u8 = 0;
pub const SIDE_SHORT: u8 = 1;
