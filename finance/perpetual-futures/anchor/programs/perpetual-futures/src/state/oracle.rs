use anchor_lang::prelude::*;
use solana_sysvar::last_restart_slot::LastRestartSlot;

use crate::constants::{BASIS_POINTS_DENOMINATOR, MAX_PRICE_STALENESS_SLOTS};
use crate::errors::PerpError;

// Byte layout of the feed account this program reads. It matches the
// `mock_switchboard::MockFeed` account: an 8-byte Anchor discriminator followed
// by `authority: Address (32)`, `price: i128 (16)`, `scale: u32 (4)`,
// `last_update_slot: u64 (8)`, `confidence: u64 (8)`.
//
// We read the raw bytes rather than deserializing the mock account type so this
// program stays decoupled from the mock. To consume a real Switchboard
// On-Demand feed, replace the offsets below with a call to
// `switchboard_on_demand::PullFeedAccountData::parse_and_verify(...)`, which
// also checks the Ed25519 signatures over the price update — the only other
// change is the feed account's owning program ID.
//
// A real feed reports a value plus a `confidence` band (a standard-deviation-like
// uncertainty). This reader rejects a price whose band is too wide relative to
// the price — skipping that check is the most common oracle footgun. Production
// venues (see solana-labs/perpetuals) often also use the feed's EMA rather than
// the spot price for a less manipulable mark; the mock omits the EMA to stay
// minimal.
//
// The feed account's owning program is NOT checked here: the pool trusts
// whatever feed address its creator configured, which is inside the trust model
// (the creator picks the oracle). A production reader must also verify the
// account owner is the oracle program, which `parse_and_verify` does.
const PRICE_OFFSET: usize = 8 + 32;
const SCALE_OFFSET: usize = PRICE_OFFSET + 16;
const LAST_UPDATE_SLOT_OFFSET: usize = SCALE_OFFSET + 4;
const CONFIDENCE_OFFSET: usize = LAST_UPDATE_SLOT_OFFSET + 8;
const FEED_MINIMUM_LENGTH: usize = CONFIDENCE_OFFSET + 8;

/// Read and validate the oracle price from `feed`.
///
/// Returns the price as a `u64` in the pool's `expected_scale` fixed point.
/// Rejects a stale price (older than `MAX_PRICE_STALENESS_SLOTS`), a
/// non-positive price, a feed whose scale differs from the pool's pinned scale,
/// and a price whose confidence band exceeds `max_confidence_bps` of the price.
pub fn read_oracle_price(
    feed: &AccountView,
    expected_scale: u32,
    max_confidence_bps: u16,
) -> Result<u64> {
    let data = feed.try_borrow_data()?;
    require!(
        data.len() >= FEED_MINIMUM_LENGTH,
        PerpError::OracleDataTooShort
    );

    let price = i128::from_le_bytes(
        data[PRICE_OFFSET..PRICE_OFFSET + 16]
            .try_into()
            .map_err(|_| PerpError::OracleDataTooShort)?,
    );
    let scale = u32::from_le_bytes(
        data[SCALE_OFFSET..SCALE_OFFSET + 4]
            .try_into()
            .map_err(|_| PerpError::OracleDataTooShort)?,
    );
    let last_update_slot = u64::from_le_bytes(
        data[LAST_UPDATE_SLOT_OFFSET..LAST_UPDATE_SLOT_OFFSET + 8]
            .try_into()
            .map_err(|_| PerpError::OracleDataTooShort)?,
    );
    let confidence = u64::from_le_bytes(
        data[CONFIDENCE_OFFSET..CONFIDENCE_OFFSET + 8]
            .try_into()
            .map_err(|_| PerpError::OracleDataTooShort)?,
    );

    require!(price > 0, PerpError::NonPositivePrice);
    require_eq!(scale, expected_scale, PerpError::OracleScaleMismatch);

    // `saturating_sub` floors the age at zero, so a feed slot momentarily ahead
    // of the local clock reads as fresh rather than wrapping to a huge age.
    let current_slot = Clock::get()?.slot;
    require!(
        current_slot.saturating_sub(last_update_slot) <= MAX_PRICE_STALENESS_SLOTS,
        PerpError::StalePrice
    );

    // Restart handling. A cluster halt stops the slot count but not the wall
    // clock, so after a restart a feed can look fresh in slots while its
    // price is hours old. With leverage a stale price is amplified into a
    // market-wide equity error, so reject any price stamped at or before the
    // restart slot; the pool pauses valuation until the publisher posts
    // again. Zero means the cluster has never restarted.
    let last_restart_slot = LastRestartSlot::get()?.last_restart_slot;
    require!(
        last_restart_slot == 0 || last_update_slot > last_restart_slot,
        PerpError::PricePredatesRestart
    );

    // Reject an untrustworthy price: confidence band as a fraction of price,
    // in basis points, must not exceed the pool's limit. Widen to u128 so the
    // product cannot overflow, and `price > 0` is already guaranteed.
    let confidence_bps = (confidence as u128)
        .checked_mul(BASIS_POINTS_DENOMINATOR as u128)
        .ok_or(PerpError::MathOverflow)?
        .checked_div(price as u128)
        .ok_or(PerpError::MathOverflow)?;
    require!(
        confidence_bps <= max_confidence_bps as u128,
        PerpError::OracleConfidenceTooWide
    );

    let price: u64 = price.try_into().map_err(|_| PerpError::MathOverflow)?;
    Ok(price)
}
