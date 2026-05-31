use anchor_lang::prelude::*;

/// Default crank cadence baked into `initialize_vault` when the caller passes
/// `0` for `crank_interval_seconds`. Ten minutes is short enough to make the
/// flash-crash window small in normal markets but long enough to keep TukTuk
/// task costs low.
#[constant]
pub const DEFAULT_CRANK_INTERVAL_SECONDS: u32 = 600;

/// Maximum age, in slots, of a price update `convert_if_triggered` will act on.
/// 150 slots is roughly one minute at ~400ms/slot. A Switchboard On-Demand
/// price is pulled fresh at crank time, so in normal operation the update is
/// only a few slots old; this bound makes the handler fail closed if the feed
/// has stopped updating rather than swap on a stale price.
pub const MAX_PRICE_STALENESS_SLOTS: u64 = 150;

/// 8-byte Anchor discriminator length. Anchor accounts (and Anchor
/// instructions) both prefix their serialised data with an 8-byte
/// discriminator, so this constant is shared.
pub const ANCHOR_DISCRIMINATOR_LENGTH: usize = 8;

/// Bytes the mock Switchboard feed lays out after the discriminator:
/// 32 (authority Pubkey) + 16 (price i128) + 4 (scale u32) + 8 (last_update_slot u64).
///
/// In production this is replaced by Switchboard On-Demand's `PullFeedAccountData`
/// layout — see `switchboard-on-demand` crate.
pub const MOCK_FEED_PAYLOAD_LENGTH: usize = 32 + 16 + 4 + 8;
