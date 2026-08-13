use anchor_lang::prelude::*;

#[derive(InitSpace, Clone, Copy, PartialEq, Eq, Debug, IdlType, wincode::SchemaRead, wincode::SchemaWrite)]
pub enum Side {
    Long,
    Short,
}

impl Side {
    /// Seed fragment used in the position PDA, so one owner can hold a long and
    /// a short in the same pool simultaneously. Returns a `'static` slice so it
    /// can be used directly in the `seeds` constraint without a temporary.
    pub fn as_seed(&self) -> &'static [u8] {
        match self {
            Side::Long => b"long",
            Side::Short => b"short",
        }
    }
}

/// A single trader's leveraged position. One PDA per (pool, owner, side).
#[account(borsh)]
#[derive(InitSpace)]
pub struct Position {
    pub owner: Address,

    pub pool: Address,

    pub side: Side,

    /// Collateral the trader posted, in collateral base units. Part of the
    /// pool's `total_collateral` while the position is open.
    pub collateral: u64,

    /// Notional position size, in collateral base units. `size / collateral` is
    /// the leverage.
    pub size: u64,

    /// Oracle price at open, in the pool's `oracle_scale` fixed point. Always
    /// positive, so stored unsigned.
    pub entry_price: u64,

    /// This position's contribution to the pool's `*_size_scaled` accumulator
    /// (`size * SIZE_PRECISION / entry_price`). Stored so it can be subtracted
    /// exactly on close without recomputing and re-rounding.
    pub size_scaled: u128,

    /// Pool `cumulative_funding` at open. Funding owed is the change since.
    pub entry_funding: i128,

    pub bump: u8,
}
