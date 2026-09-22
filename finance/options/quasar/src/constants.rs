/// Basis-point denominator: 100% = 10_000 bps. The venue's fee on each premium
/// is expressed in basis points and divided by this.
pub const BASIS_POINTS_DENOMINATOR: u64 = 10_000;

/// `kind` values. Quasar instruction arguments and zero-copy fields are plain
/// integers, so the Anchor sibling's `OptionKind` enum becomes a `u8`.
pub const KIND_CALL: u8 = 0;
pub const KIND_PUT: u8 = 1;

/// `status` values, mirroring the Anchor sibling's `OptionStatus`.
pub const STATUS_LISTED: u8 = 0;
pub const STATUS_HELD: u8 = 1;
pub const STATUS_EXERCISED: u8 = 2;
