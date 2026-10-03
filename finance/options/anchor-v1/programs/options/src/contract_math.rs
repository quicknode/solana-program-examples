//! The pure arithmetic of one option, separated from account handling so it
//! can be unit-tested and model-checked (see `finance/options/kani-proofs`)
//! without the Solana machinery.
//!
//! Settlement does no arithmetic at all: the option stores the two amounts
//! that change hands on exercise, so nothing is multiplied, divided or
//! rounded. The only rounding in the program is the floor in the fee split,
//! which returns `None` on the paths the program maps to
//! `OptionsError::MathOverflow`.

use crate::state::OptionKind;

/// Basis-point denominator, mirroring `constants::BASIS_POINTS_DENOMINATOR`.
const BASIS_POINTS: u128 = 10_000;

/// What the writer posts, in the collateral token's minor units: the
/// underlying for a call, the strike for a put. Whatever the holder is
/// entitled to at exercise is sitting in the vault from the moment the option
/// exists, which is what makes the option fully collateralized.
pub fn collateral_amount(kind: OptionKind, underlying_amount: u64, strike_amount: u64) -> u64 {
    match kind {
        OptionKind::Call => underlying_amount,
        OptionKind::Put => strike_amount,
    }
}

/// What the holder pays at exercise, and the writer later collects: the
/// strike for a call, the underlying for a put. The mirror of
/// `collateral_amount`, in the other token.
pub fn exercise_payment(kind: OptionKind, underlying_amount: u64, strike_amount: u64) -> u64 {
    match kind {
        OptionKind::Call => strike_amount,
        OptionKind::Put => underlying_amount,
    }
}

/// Split a premium into the venue's fee and the writer's share. The fee
/// floors, so the writer receives the rounding minor unit; the venue gives up
/// at most one minor unit per sale, and a sale needs a real premium, so the
/// leak cannot be industrialized.
pub fn split_premium(premium: u64, fee_bps: u16) -> Option<(u64, u64)> {
    let fee = (premium as u128)
        .checked_mul(fee_bps as u128)?
        .checked_div(BASIS_POINTS)?;
    let fee = u64::try_from(fee).ok()?;
    let to_writer = premium.checked_sub(fee)?;
    Some((fee, to_writer))
}

/// The holder may exercise while the option has not expired.
pub fn may_exercise(now: i64, expiry: i64) -> bool {
    now < expiry
}

/// The writer may reclaim collateral once the option has expired: the exact
/// complement of `may_exercise`, so there is no instant at which both the
/// holder and the writer can claim the same collateral, and none at which
/// neither can.
pub fn may_reclaim(now: i64, expiry: i64) -> bool {
    now >= expiry
}
