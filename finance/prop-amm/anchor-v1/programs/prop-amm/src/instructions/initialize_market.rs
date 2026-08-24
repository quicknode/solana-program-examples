use anchor_lang::prelude::*;
use anchor_spl::{
    associated_token::AssociatedToken,
    token_interface::{Mint, TokenAccount, TokenInterface},
};

use crate::constants::{
    AUTHORITY_SEED, BASE_VAULT_SEED, BASIS_POINTS_DENOMINATOR, MARKET_SEED, QUOTE_VAULT_SEED,
};
use crate::errors::PropAmmError;
use crate::state::Market;

/// Quote parameters set at market creation. Bundled into one struct so the
/// instruction signature stays readable.
#[derive(AnchorSerialize, AnchorDeserialize, Clone)]
pub struct MarketParameters {
    /// Decimal places the oracle quotes its price in (e.g. 8).
    pub oracle_scale: u32,

    /// Half-spread in basis points: ask = oracle + spread, bid = oracle - spread.
    pub spread_bps: u16,

    /// Maximum oracle confidence band tolerated, in basis points of the price.
    pub max_confidence_bps: u16,
}

pub fn handle_initialize_market(
    context: Context<InitializeMarketAccountConstraints>,
    parameters: MarketParameters,
) -> Result<()> {
    let denominator = BASIS_POINTS_DENOMINATOR as u16;
    // A market quoting the same token against itself prices nothing.
    require_keys_neq!(
        context.accounts.base_mint.key(),
        context.accounts.quote_mint.key(),
        PropAmmError::InvalidParameter
    );
    // Zero spread means quoting the oracle price for free while paying adverse
    // selection on every fill — almost certainly a configuration mistake, so
    // it is rejected rather than allowed to bleed. At or above 100% the bid
    // goes to zero or below and the quote stops meaning anything.
    require!(
        parameters.spread_bps > 0 && parameters.spread_bps < denominator,
        PropAmmError::InvalidParameter
    );
    // Zero would reject every real feed (which always reports some uncertainty);
    // above 100% is meaningless. Anything in between is a valid risk choice.
    require!(
        parameters.max_confidence_bps > 0 && parameters.max_confidence_bps < denominator,
        PropAmmError::InvalidParameter
    );

    let market = &mut context.accounts.market;
    market.operator = context.accounts.operator.key();
    market.base_mint = context.accounts.base_mint.key();
    market.quote_mint = context.accounts.quote_mint.key();
    market.oracle_feed = context.accounts.oracle_feed.key();
    market.base_vault = context.accounts.base_vault.key();
    market.quote_vault = context.accounts.quote_vault.key();
    market.oracle_scale = parameters.oracle_scale;
    market.base_decimals = context.accounts.base_mint.decimals;
    market.quote_decimals = context.accounts.quote_mint.decimals;
    market.spread_bps = parameters.spread_bps;
    market.max_confidence_bps = parameters.max_confidence_bps;
    market.paused = false;
    market.bump = context.bumps.market;
    market.authority_bump = context.bumps.market_authority;

    Ok(())
}

#[derive(Accounts)]
pub struct InitializeMarketAccountConstraints<'info> {
    #[account(mut)]
    pub operator: Signer<'info>,

    // One market per pair: the deployment IS the firm. A real prop AMM is a
    // closed program deployed by the market-making firm itself, so there is no
    // reason for two markets in the same pair to coexist in one deployment.
    #[account(
        init,
        payer = operator,
        space = Market::DISCRIMINATOR.len() + Market::INIT_SPACE,
        seeds = [MARKET_SEED, base_mint.key().as_ref(), quote_mint.key().as_ref()],
        bump,
    )]
    pub market: Box<Account<'info, Market>>,

    pub base_mint: Box<InterfaceAccount<'info, Mint>>,

    pub quote_mint: Box<InterfaceAccount<'info, Mint>>,

    /// CHECK: The oracle feed account. Its key is stored on the market and
    /// every read validates the layout, scale, and freshness; it is never
    /// trusted by type. Swap for a real Switchboard feed in production.
    pub oracle_feed: UncheckedAccount<'info>,

    /// CHECK: PDA that owns both vaults. Holds no data; used only to sign
    /// vault CPIs.
    #[account(
        seeds = [AUTHORITY_SEED, market.key().as_ref()],
        bump,
    )]
    pub market_authority: UncheckedAccount<'info>,

    #[account(
        init,
        payer = operator,
        seeds = [BASE_VAULT_SEED, market.key().as_ref()],
        bump,
        token::mint = base_mint,
        token::authority = market_authority,
        token::token_program = token_program,
    )]
    pub base_vault: Box<InterfaceAccount<'info, TokenAccount>>,

    #[account(
        init,
        payer = operator,
        seeds = [QUOTE_VAULT_SEED, market.key().as_ref()],
        bump,
        token::mint = quote_mint,
        token::authority = market_authority,
        token::token_program = token_program,
    )]
    pub quote_vault: Box<InterfaceAccount<'info, TokenAccount>>,

    pub token_program: Interface<'info, TokenInterface>,
    pub associated_token_program: Program<'info, AssociatedToken>,
    pub system_program: Program<'info, System>,
}
