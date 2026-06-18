//! Program accounts. Quasar accounts are zero-copy; fixed-size fields only
//! (no `Vec`), which is why this Quasar port models an isolated single-collateral,
//! single-borrow position per obligation rather than the Anchor version's
//! multi-asset obligation.

use quasar_lang::prelude::*;

/// Top-level market config. PDA: `["lending_market", owner]`.
#[account(discriminator = 1, set_inner)]
#[seeds(b"lending_market", owner: Address)]
pub struct LendingMarket {
    pub owner: Address,
    pub quote_mint: Address,
    pub bump: u8,
}

/// One asset's pool. PDA: `["reserve", lending_market, liquidity_mint]`.
/// The reserve PDA is the authority of both `liquidity_vault` and `share_mint`.
#[account(discriminator = 2, set_inner)]
#[seeds(b"reserve", lending_market: Address, liquidity_mint: Address)]
pub struct Reserve {
    pub lending_market: Address,
    pub liquidity_mint: Address,
    pub liquidity_vault: Address,
    pub share_mint: Address,
    pub price_feed: Address,
    pub available_liquidity: u64,
    pub share_mint_supply: u64,
    /// Liquidity owed to the market owner: the protocol's cut of accrued
    /// interest, carved out of total liquidity and withdrawn via
    /// `collect_protocol_fees`.
    pub accumulated_protocol_fees: u64,
    pub borrowed_amount_scaled: u128,
    pub cumulative_borrow_rate_index: u128,
    pub last_update_slot: u64,
    pub liquidity_decimals: u8,
    pub loan_to_value_bps: u16,
    pub liquidation_threshold_bps: u16,
    pub liquidation_bonus_bps: u16,
    pub close_factor_bps: u16,
    /// Share of accrued borrow interest kept by the protocol (how the owner earns).
    pub reserve_factor_bps: u16,
    pub optimal_utilization_bps: u16,
    pub min_borrow_rate_bps: u16,
    pub optimal_borrow_rate_bps: u16,
    pub max_borrow_rate_bps: u16,
    pub bump: u8,
}

/// A borrower's isolated position. PDA: `["obligation", lending_market, owner]`.
/// `collateral_reserve` / `borrow_reserve` are the zero address until first used.
#[account(discriminator = 3, set_inner)]
#[seeds(b"obligation", lending_market: Address, owner: Address)]
pub struct Obligation {
    pub lending_market: Address,
    pub owner: Address,
    pub collateral_reserve: Address,
    pub deposited_shares: u64,
    pub borrow_reserve: Address,
    pub borrowed_scaled: u128,
    pub bump: u8,
}

/// Switchboard-On-Demand-shaped price feed. PDA: `["price_feed", authority, mint]`
/// — the writer is part of the address, so no two authorities can contend for
/// the same feed, and a reserve trusts exactly the feed its market owner passed
/// to `init_reserve`. `price = price_mantissa * 10^exponent`; freshness is
/// checked in slots. In production this account would be the real Switchboard feed.
#[account(discriminator = 4, set_inner)]
#[seeds(b"price_feed", authority: Address, mint: Address)]
pub struct PriceFeed {
    pub mint: Address,
    pub price_mantissa: i128,
    pub exponent: i32,
    pub last_updated_slot: u64,
    pub authority: Address,
    pub bump: u8,
}

/// PDA marker for a reserve's liquidity vault: `["liquidity_vault", reserve]`.
#[derive(Seeds)]
#[seeds(b"liquidity_vault", reserve: Address)]
pub struct LiquidityVaultPda;

/// PDA marker for a reserve's share mint: `["share_mint", reserve]`.
#[derive(Seeds)]
#[seeds(b"share_mint", reserve: Address)]
pub struct ShareMintPda;

/// PDA marker for an obligation's collateral vault: `["obligation_vault", reserve, obligation]`.
#[derive(Seeds)]
#[seeds(b"obligation_vault", reserve: Address, obligation: Address)]
pub struct ObligationVaultPda;
