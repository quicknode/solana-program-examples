use quasar_lang::prelude::*;

/// One perpetual-futures market. Mirrors the Anchor `Pool` field-for-field; see
/// the Anchor sibling's README for what each field means. Money fields are raw
/// base units of the collateral token.
#[account(discriminator = 100, set_inner)]
#[seeds(b"pool", collateral_mint: Address, oracle_feed: Address)]
pub struct Pool {
    pub authority: Address,
    pub collateral_mint: Address,
    pub oracle_feed: Address,
    pub custody_vault: Address,
    pub lp_mint: Address,
    pub oracle_scale: u32,
    pub liquidity: u64,
    /// Senior buffer that absorbs a bankrupt position's deficit before the loss
    /// is socialized to liquidity providers, and counts alongside `liquidity` as
    /// backing for trader profit in the haircut math. Funded by
    /// `insurance_fee_bps` of every open/close fee.
    pub insurance_fund: u64,
    pub total_collateral: u64,
    pub protocol_fees: u64,
    pub long_size: u128,
    pub short_size: u128,
    pub long_size_scaled: u128,
    pub short_size_scaled: u128,
    pub cumulative_funding: i128,
    pub last_funding_slot: u64,
    pub funding_rate_per_slot: u64,
    pub open_fee_bps: u16,
    pub close_fee_bps: u16,
    pub max_leverage: u16,
    pub maintenance_margin_bps: u16,
    pub liquidation_fee_bps: u16,
    /// Maximum oracle confidence band, in basis points of the price, the pool
    /// will trade against. A wider band is rejected as untrustworthy.
    pub max_confidence_bps: u16,
    /// Fraction of each open/close fee, in basis points, routed to the insurance
    /// fund instead of to `protocol_fees`.
    pub insurance_fee_bps: u16,
    /// Slots a position must stay open before its profit matures into a
    /// withdrawable claim. Profit cannot be realized before this elapses, the
    /// oracle-manipulation defense; loss is never gated this way.
    pub profit_warmup_slots: u64,
    pub bump: u8,
    pub authority_bump: u8,
}

/// One trader's leveraged position, one PDA per (pool, owner). Unlike the Anchor
/// sibling — which seeds the position by side so a trader can hold a long and a
/// short at once — Quasar's `address` constraint can only reference account
/// inputs, not instruction arguments, so `side` is stored in the account rather
/// than used as a seed. A trader therefore holds one position per pool here.
#[account(discriminator = 101, set_inner)]
#[seeds(b"position", pool: Address, owner: Address)]
pub struct Position {
    pub owner: Address,
    pub pool: Address,
    pub side: u8,
    pub collateral: u64,
    pub size: u64,
    pub entry_price: u64,
    pub size_scaled: u128,
    pub entry_funding: i128,
    /// Slot the position opened at. Its profit matures once
    /// `entry_slot + pool.profit_warmup_slots` has passed.
    pub entry_slot: u64,
    pub bump: u8,
}
