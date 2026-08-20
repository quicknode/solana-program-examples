use anchor_lang::prelude::*;

/// Which side of the quote a swap takes.
#[derive(Clone, Copy, PartialEq, Eq, Debug, IdlType, wincode::SchemaRead, wincode::SchemaWrite)]
pub enum Direction {
    /// Spend the quote token, receive the base token, priced at the ask
    /// (oracle plus spread).
    BuyBase,
    /// Spend the base token, receive the quote token, priced at the bid
    /// (oracle minus spread).
    SellBase,
}

/// One quoted market: a base/quote token pair priced by one oracle feed, with
/// inventory owned entirely by one operator.
///
/// Note what this account does NOT hold, compared to a curve AMM's pool: no
/// liquidity-provider mint, no fee ledger, no reserves that pricing depends
/// on. The price comes from the oracle; the vault balances only bound how much
/// of a fill is possible. And because nobody but the operator has a claim on
/// the vaults, the token balances themselves are the complete accounting.
#[account(borsh)]
#[derive(InitSpace)]
pub struct Market {
    /// The market-making firm. Deposits and withdraws inventory, sets the
    /// spread, pauses quoting. Cannot touch anyone else's funds, because the
    /// market never holds anyone else's funds.
    pub operator: Address,

    pub base_mint: Address,

    pub quote_mint: Address,

    /// Oracle feed this market quotes from. Stored so handlers can reject any
    /// substituted feed account.
    pub oracle_feed: Address,

    pub base_vault: Address,

    pub quote_vault: Address,

    /// Decimal places the oracle price is quoted in. Pinned at creation so a
    /// feed that silently changes scale is rejected rather than mis-read.
    pub oracle_scale: u32,

    /// Decimals of the two mints, pinned at creation so the quote math never
    /// has to trust a passed-in mint account for them.
    pub base_decimals: u8,

    pub quote_decimals: u8,

    /// Half-spread in basis points: the ask is the oracle price plus this, the
    /// bid is the oracle price minus it. The spread is the operator's entire
    /// revenue — there is no separate fee.
    pub spread_bps: u16,

    /// Maximum oracle confidence band, in basis points of the price, that the
    /// market will quote against. A wider band is rejected as untrustworthy.
    pub max_confidence_bps: u16,

    /// True while the operator has pulled its quotes. Swaps are rejected;
    /// inventory operations still work.
    pub paused: bool,

    pub bump: u8,

    /// Bump for the vault authority PDA, stored so CPIs can sign without
    /// re-deriving it.
    pub authority_bump: u8,
}
