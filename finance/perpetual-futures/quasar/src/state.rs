use quasar_lang::prelude::*;

/// One perpetual-futures market. Mirrors the Anchor `Pool` field-for-field; see
/// the Anchor sibling's README for what each field means. Money fields are raw
/// base units of the collateral token.
/// The pool account owns the custody vault and is the liquidity-provider
/// mint's authority; it signs vault transfers and mint/burn CPIs with its own
/// seeds. There is no separate signing PDA.
#[account(discriminator = 100, set_inner)]
#[seeds(b"pool", collateral_mint: Address, oracle_feed: Address)]
pub struct Pool {
    pub authority: Address,
    pub collateral_mint: Address,
    pub oracle_feed: Address,
    pub custody_vault: Address,
    pub lp_mint: Address,
    pub oracle_scale: u32,
    /// Liquidity-provider-owned tokens. Together with `insurance_fund` it backs
    /// trader profit: when the two cannot cover the profit traders are owed,
    /// every closing winner is paid the same fraction of their profit (see
    /// `instructions::shared::haircut_ratio`).
    pub liquidity: u64,
    pub total_collateral: u64,
    pub program_fees: u64,
    /// Funded by `insurance_fee_bps` of every open and close fee. It pays a
    /// bankrupt position's deficit (its loss beyond its collateral) before
    /// liquidity providers bear any of it, pays a winner's profit once
    /// `liquidity` is exhausted, and counts alongside `liquidity` as backing in
    /// the haircut. The vault holds `liquidity + total_collateral +
    /// program_fees + insurance_fund`, plus any tokens sent to it directly.
    pub insurance_fund: u64,
    pub long_size: u128,
    pub short_size: u128,
    pub long_size_scaled: u128,
    pub short_size_scaled: u128,
    pub cumulative_funding: i128,
    /// The Clock's `unix_timestamp` when funding last accrued. Funding runs on
    /// the wall clock, so what a position costs per hour does not depend on
    /// the cluster's slot time.
    pub last_funding_timestamp: i64,
    /// Time-weighted moving average of the oracle price, in the pool's
    /// `oracle_scale` fixed point. Seeded with the oracle price when the pool is
    /// created. Every handler that reads the oracle credits the seconds since
    /// the previous read to `last_oracle_price`, the price that read saw.
    /// Trading and liquidity handlers refuse an oracle price more than
    /// `max_price_deviation_bps` away from it, so a sudden jump pauses them
    /// until the average catches up.
    pub average_price: u64,
    /// The oracle price at the most recent read, in `oracle_scale` fixed point.
    /// The next read folds it into `average_price` for the seconds in between.
    pub last_oracle_price: u64,
    /// The Clock's `unix_timestamp` of the most recent fold into
    /// `average_price`.
    pub average_price_timestamp: i64,
    /// Funding accrued per second, in `FUNDING_PRECISION` units, applied to the
    /// heavier side. The funding paid by traders accrues to the pool.
    pub funding_rate_per_second: u64,
    pub open_fee_bps: u16,
    pub close_fee_bps: u16,
    /// Net collateral a position must post to open, in basis points of its
    /// notional size: 1_000 allows at most 10x leverage. Always above
    /// `maintenance_margin_bps`, so no position opens already liquidatable.
    pub initial_margin_bps: u16,
    pub maintenance_margin_bps: u16,
    pub liquidation_fee_bps: u16,
    /// Maximum oracle confidence band, in basis points of the price, the pool
    /// will trade against. A wider band is rejected as untrustworthy.
    pub max_confidence_bps: u16,
    /// Widest gap the pool trades across between the oracle price and
    /// `average_price`, in basis points of `average_price`.
    pub max_price_deviation_bps: u16,
    /// Fraction of each open and close fee, in basis points, paid into
    /// `insurance_fund`; the rest goes to `program_fees`.
    pub insurance_fee_bps: u16,
    /// Slots a position must stay open before `close_position` will pay it a
    /// profit. Someone who pushes the oracle to a false price cannot open a
    /// position and take its profit less than this many slots apart; by then the
    /// price has had that long to correct. A losing position can close, and an
    /// under-margined one be liquidated, at any time.
    pub profit_warmup_slots: u64,
    pub bump: u8,
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
    /// Slot the position opened in. `close_position` pays a profit only from
    /// slot `entry_slot + pool.profit_warmup_slots` on.
    pub entry_slot: u64,
    pub bump: u8,
}
