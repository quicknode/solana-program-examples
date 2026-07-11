use quasar_lang::prelude::*;

/// One quoted market. Mirrors the Anchor `Market` field-for-field; see the
/// Anchor sibling's README for what each field means. `paused` is a `u8`
/// (0 = quoting, 1 = pulled) because the account layout is zero-copy.
///
/// Note what this account does NOT hold, compared to a curve AMM's pool: no
/// liquidity-provider mint, no fee ledger, no reserves that pricing depends
/// on. The operator is the only capital in the market.
#[account(discriminator = 100, set_inner)]
#[seeds(b"market", base_mint: Address, quote_mint: Address)]
pub struct Market {
    pub operator: Address,
    pub base_mint: Address,
    pub quote_mint: Address,
    pub oracle_feed: Address,
    pub base_vault: Address,
    pub quote_vault: Address,
    /// Decimal places the oracle price is quoted in, pinned at creation.
    pub oracle_scale: u32,
    pub base_decimals: u8,
    pub quote_decimals: u8,
    /// Half-spread in basis points: ask = oracle + spread, bid = oracle - spread.
    /// The spread is the operator's entire revenue — there is no separate fee.
    pub spread_bps: u16,
    /// Maximum oracle confidence band, in basis points of the price, the
    /// market will quote against.
    pub max_confidence_bps: u16,
    /// 1 while the operator has pulled its quotes; swaps are rejected.
    pub paused: u8,
    pub bump: u8,
    pub authority_bump: u8,
}
