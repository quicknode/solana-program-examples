use anchor_lang::prelude::*;

/// Basis-point denominator: 100% = 10_000 bps. All fee and margin parameters are
/// expressed in basis points and divided by this.
#[constant]
pub const BASIS_POINTS_DENOMINATOR: u64 = 10_000;

/// Fixed-point precision for the cumulative funding index. The index is carried
/// as `i128` scaled by this factor so per-second funding (a tiny ratio) keeps its
/// precision when integrated over many seconds.
pub const FUNDING_PRECISION: i128 = 1_000_000_000;

/// Fixed-point precision for the aggregate `size / entry_price` accumulators the
/// pool keeps per side. Lets mark-to-market assets-under-management be computed
/// from two running sums instead of iterating every open position.
pub const SIZE_PRECISION: u128 = 1_000_000_000;

/// Liquidity-provider shares withheld from the first deposit. The first
/// depositor receives `deposit - MINIMUM_LIQUIDITY` shares rather than the full
/// amount, the same convention Uniswap V2 uses, and both `add_liquidity` and
/// `remove_liquidity` divide by the share supply plus this minimum, so the
/// withheld shares belong to nobody and their slice of the pool never leaves.
/// Share value is priced off tracked liquidity, not the vault token balance,
/// so a direct donation to the vault cannot move it; but a provider who is also
/// the only trader can grow `liquidity` with their own funding payments and
/// losses, and it is the locked minimum that makes that cost them.
#[constant]
pub const MINIMUM_LIQUIDITY: u64 = 1_000;

/// Reject an oracle price older than this many slots. Slot count is what the
/// runtime guarantees; unix timestamps are validator-influenced. How long the
/// window is in seconds follows the cluster's slot time, which the protocol
/// lowers over time, so the window tightens on its own and never loosens.
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
#[constant]
pub const PRICE_AVERAGE_WINDOW_SECONDS: i64 = 600;

/// Upper bound on the per-pool `funding_rate_per_second` parameter, in
/// `FUNDING_PRECISION` units: 277 billionths of a position's size per second,
/// just under 0.1% of its size per hour. The rate is fixed when the pool is
/// created, so everyone who opens a position or deposits liquidity has seen it,
/// and no position can be charged or paid funding faster than this.
#[constant]
pub const MAX_FUNDING_RATE_PER_SECOND: u64 = 277;

#[constant]
pub const POOL_SEED: &[u8] = b"pool";

#[constant]
pub const LP_MINT_SEED: &[u8] = b"lp_mint";

#[constant]
pub const VAULT_SEED: &[u8] = b"vault";

#[constant]
pub const POSITION_SEED: &[u8] = b"position";
