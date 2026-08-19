/// Basis-point denominator: 100% = 10_000 bps.
pub const BASIS_POINTS_DENOMINATOR: u64 = 10_000;

/// Reject an oracle price older than this many slots. Counted in slots because
/// the runtime guarantees slot progression; the seconds that comes to follow the
/// cluster's slot time, which the protocol lowers over time. For a market maker
/// the bound is the business itself: a stale quote is a free option for whoever
/// notices first.
pub const MAX_PRICE_STALENESS_SLOTS: u64 = 150;

/// `direction` argument values for `swap`. Quasar instruction arguments are
/// plain integers, so the Anchor sibling's `Direction` enum becomes a `u8`.
pub const DIRECTION_BUY_BASE: u8 = 0;
pub const DIRECTION_SELL_BASE: u8 = 1;
