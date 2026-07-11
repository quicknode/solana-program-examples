use {
    crate::{
        constants::BASIS_POINTS_DENOMINATOR,
        instructions::shared::{err, error},
        state::{Market, MarketInner},
        BaseVaultPda, MarketAuthorityPda, QuoteVaultPda,
    },
    quasar_lang::prelude::*,
    quasar_spl::prelude::*,
};

#[derive(Accounts)]
pub struct InitializeMarket {
    #[account(mut)]
    pub operator: Signer,
    // One market per pair: the deployment IS the firm. A real prop AMM is a
    // closed program deployed by the market-making firm itself.
    #[account(
        mut,
        init,
        payer = operator,
        address = Market::seeds(base_mint.address(), quote_mint.address()),
    )]
    pub market: Account<Market>,
    pub base_mint: Account<Mint>,
    pub quote_mint: Account<Mint>,
    /// CHECK: stored on the market; every read validates layout, scale,
    /// freshness, and confidence.
    pub oracle_feed: UncheckedAccount,
    /// Authority PDA over both vaults. Holds no data; only signs.
    #[account(address = MarketAuthorityPda::seeds(market.address()))]
    pub market_authority: UncheckedAccount,
    #[account(
        mut,
        init(idempotent),
        payer = operator,
        address = BaseVaultPda::seeds(market.address()),
        token(mint = base_mint, authority = market_authority, token_program = token_program),
    )]
    pub base_vault: Account<Token>,
    #[account(
        mut,
        init(idempotent),
        payer = operator,
        address = QuoteVaultPda::seeds(market.address()),
        token(mint = quote_mint, authority = market_authority, token_program = token_program),
    )]
    pub quote_vault: Account<Token>,
    pub token_program: Program<TokenProgram>,
    pub system_program: Program<SystemProgram>,
    pub rent: Sysvar<Rent>,
}

#[inline(always)]
pub fn handle_initialize_market(
    accounts: &mut InitializeMarket,
    oracle_scale: u32,
    spread_bps: u16,
    max_confidence_bps: u16,
    bumps: &InitializeMarketBumps,
) -> Result<(), ProgramError> {
    let denominator = BASIS_POINTS_DENOMINATOR as u16;
    // A market quoting the same token against itself prices nothing.
    if accounts.base_mint.address() == accounts.quote_mint.address() {
        return Err(err(error::INVALID_PARAMETER));
    }
    // Zero spread quotes the oracle price for free while paying adverse
    // selection on every fill; at or above 100% the bid stops meaning anything.
    if spread_bps == 0 || spread_bps >= denominator {
        return Err(err(error::INVALID_PARAMETER));
    }
    // Zero would reject every real feed; above 100% is meaningless.
    if max_confidence_bps == 0 || max_confidence_bps >= denominator {
        return Err(err(error::INVALID_PARAMETER));
    }

    let base_decimals = accounts.base_mint.decimals();
    let quote_decimals = accounts.quote_mint.decimals();
    accounts.market.set_inner(MarketInner {
        operator: *accounts.operator.address(),
        base_mint: *accounts.base_mint.address(),
        quote_mint: *accounts.quote_mint.address(),
        oracle_feed: *accounts.oracle_feed.address(),
        base_vault: *accounts.base_vault.address(),
        quote_vault: *accounts.quote_vault.address(),
        oracle_scale,
        base_decimals,
        quote_decimals,
        spread_bps,
        max_confidence_bps,
        paused: 0,
        bump: bumps.market,
        authority_bump: bumps.market_authority,
    });
    Ok(())
}
