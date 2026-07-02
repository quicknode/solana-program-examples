use anchor_lang::prelude::*;

/// Basis-point denominator: 100% = 10_000 bps. All fee and margin parameters are
/// expressed in basis points and divided by this.
#[constant]
pub const BASIS_POINTS_DENOMINATOR: u64 = 10_000;

/// Fixed-point precision for the cumulative funding index. The index is carried
/// as `i128` scaled by this factor so per-slot funding (a tiny ratio) keeps its
/// precision when integrated over many slots.
pub const FUNDING_PRECISION: i128 = 1_000_000_000;

/// Fixed-point precision for the aggregate `size / entry_price` accumulators the
/// pool keeps per side. Lets mark-to-market assets-under-management be computed
/// from two running sums instead of iterating every open position.
pub const SIZE_PRECISION: u128 = 1_000_000_000;

/// Fixed-point precision for the haircut ratio `h`. The ratio is carried scaled
/// by this factor: `HAIRCUT_PRECISION` means `h = 1` (profit fully backed), and a
/// smaller value means profit is honoured only in proportion. Profit (a junior
/// claim) is multiplied by `h` and divided by this on the way out, rounding down
/// so the haircut payouts can never sum to more than the pool actually holds.
pub const HAIRCUT_PRECISION: u128 = 1_000_000_000;

/// Liquidity-provider shares withheld from the first deposit. The first
/// depositor receives `deposit - MINIMUM_LIQUIDITY` shares rather than the full
/// amount, the same convention Uniswap V2 uses, so the share supply can never be
/// driven to a dust amount that rounding could exploit. (Share value here is
/// priced off tracked liquidity, not the vault token balance, so a direct
/// donation to the vault cannot move it.)
#[constant]
pub const MINIMUM_LIQUIDITY: u64 = 1_000;

/// Reject an oracle price older than this many slots. Slot count is what the
/// runtime guarantees; unix timestamps are validator-influenced. ~150 slots is
/// roughly one minute at 400ms/slot.
pub const MAX_PRICE_STALENESS_SLOTS: u64 = 150;

/// Upper bound on the per-pool `max_leverage` parameter, so a pool cannot be
/// configured with an absurd leverage that makes every position instantly
/// liquidatable on the smallest price move.
pub const MAX_LEVERAGE_CEILING: u16 = 100;

#[constant]
pub const POOL_SEED: &[u8] = b"pool";

#[constant]
pub const AUTHORITY_SEED: &[u8] = b"authority";

#[constant]
pub const LP_MINT_SEED: &[u8] = b"lp_mint";

#[constant]
pub const VAULT_SEED: &[u8] = b"vault";

#[constant]
pub const POSITION_SEED: &[u8] = b"position";
