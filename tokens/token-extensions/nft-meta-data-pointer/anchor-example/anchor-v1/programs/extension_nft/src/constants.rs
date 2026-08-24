pub const TIME_TO_REFILL_ENERGY: i64 = 60;
pub const MAX_ENERGY: u64 = 100;
pub const MAX_WOOD_PER_TREE: u64 = 100000;

/// Rough over-allocation for the inline SPL Token Metadata extension TLV
/// appended to the Mint account. The TLV is dynamic (name / symbol / uri /
/// key-value additional fields), so we cannot derive it via `InitSpace`.
/// 250 bytes is enough headroom for our fixture NFTs - raise if you add
/// longer strings or many extra fields.
pub const TOKEN_METADATA_EXTENSION_SPACE: usize = 250;
