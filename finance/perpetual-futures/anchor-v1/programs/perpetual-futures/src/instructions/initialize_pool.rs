use anchor_lang::prelude::*;
use anchor_spl::{
    associated_token::AssociatedToken,
    token_interface::{Mint, TokenAccount, TokenInterface},
};

use crate::constants::{
    BASIS_POINTS_DENOMINATOR, LP_MINT_SEED, MAX_FUNDING_RATE_PER_SECOND, POOL_SEED, VAULT_SEED,
};
use crate::errors::PerpError;
use crate::state::oracle::read_oracle_price;
use crate::state::Pool;

/// Trading parameters set once at pool creation. None of them can be changed
/// afterwards. Bundled into one struct so the
/// instruction signature stays readable.
#[derive(AnchorSerialize, AnchorDeserialize, Clone)]
pub struct PoolParameters {
    /// Decimal places the oracle quotes its price in (e.g. 8).
    pub oracle_scale: u32,

    /// Funding accrued per second, in `FUNDING_PRECISION` units, charged to the
    /// heavier side.
    pub funding_rate_per_second: u64,

    pub open_fee_bps: u16,
    pub close_fee_bps: u16,

    /// Net collateral a position must post to open, in basis points of its
    /// notional size. Must be above `maintenance_margin_bps` and at most
    /// 10_000 (no leverage).
    pub initial_margin_bps: u16,

    pub maintenance_margin_bps: u16,
    pub liquidation_fee_bps: u16,

    /// Maximum oracle confidence band tolerated, in basis points of the price.
    pub max_confidence_bps: u16,

    /// Widest gap, in basis points of the pool's average price, between the
    /// oracle price and that average at which positions may still open or
    /// close and liquidity may still move.
    pub max_price_deviation_bps: u16,

    /// Fraction of each open and close fee, in basis points, paid into the
    /// insurance fund; the rest goes to program fees. Must be below 10_000.
    pub insurance_fee_bps: u16,

    /// Slots a position must stay open before it can be closed at a profit.
    pub profit_warmup_slots: u64,
}

pub fn handle_initialize_pool(
    context: Context<InitializePoolAccountConstraints>,
    parameters: PoolParameters,
) -> Result<()> {
    let denominator = BASIS_POINTS_DENOMINATOR as u16;
    // The rate never changes after this, so bounding it here bounds it for the
    // life of the pool.
    require!(
        parameters.funding_rate_per_second <= MAX_FUNDING_RATE_PER_SECOND,
        PerpError::InvalidParameter
    );
    require!(
        parameters.open_fee_bps < denominator,
        PerpError::InvalidParameter
    );
    require!(
        parameters.close_fee_bps < denominator,
        PerpError::InvalidParameter
    );
    require!(
        parameters.liquidation_fee_bps < denominator,
        PerpError::InvalidParameter
    );
    // Maintenance margin must leave room above zero and below full notional;
    // a position is liquidatable once equity drops to this fraction of size.
    require!(
        parameters.maintenance_margin_bps > 0 && parameters.maintenance_margin_bps < denominator,
        PerpError::InvalidParameter
    );
    // close_position deducts the close fee from equity and refuses a
    // non-positive payout, while liquidation only acts at or below the
    // maintenance margin. The margin must therefore exceed the close fee, or a
    // position could be stranded in between: too healthy to liquidate, too poor
    // to pay the fee to close.
    require!(
        parameters.maintenance_margin_bps > parameters.close_fee_bps,
        PerpError::InvalidParameter
    );
    // A position must open with more margin than it is liquidated at, or it
    // could be liquidated in the same slot it opened. At most 100% of
    // notional: more than that would demand collateral above the position's
    // size.
    require!(
        parameters.initial_margin_bps > parameters.maintenance_margin_bps,
        PerpError::InitialMarginNotAboveMaintenance
    );
    require!(
        parameters.initial_margin_bps <= denominator,
        PerpError::InvalidParameter
    );
    // Zero would reject every real feed (which always reports some uncertainty);
    // above 100% is meaningless. Anything in between is a valid risk choice.
    require!(
        parameters.max_confidence_bps > 0 && parameters.max_confidence_bps < denominator,
        PerpError::InvalidParameter
    );
    // At 10_000 every fee would go to the insurance fund and none to the
    // program.
    require!(
        parameters.insurance_fee_bps < denominator,
        PerpError::InvalidParameter
    );
    // Zero would refuse every price move, however small. At 100% or more the
    // band could never refuse a fall, since the oracle price is always
    // positive.
    require!(
        parameters.max_price_deviation_bps > 0 && parameters.max_price_deviation_bps < denominator,
        PerpError::InvalidPriceDeviation
    );

    // Record the feed's owning program, and seed the average with a validated
    // oracle price, so the owner check and the band are in force from the
    // first trade.
    let price_feed_program = *context.accounts.oracle_feed.owner;
    let initial_price = read_oracle_price(
        &context.accounts.oracle_feed,
        &price_feed_program,
        parameters.oracle_scale,
        parameters.max_confidence_bps,
    )?;
    let current_timestamp = Clock::get()?.unix_timestamp;

    let pool = &mut context.accounts.pool;
    pool.authority = context.accounts.authority.key();
    pool.collateral_mint = context.accounts.collateral_mint.key();
    pool.oracle_feed = context.accounts.oracle_feed.key();
    pool.price_feed_program = price_feed_program;
    pool.oracle_scale = parameters.oracle_scale;
    pool.custody_vault = context.accounts.custody_vault.key();
    pool.lp_mint = context.accounts.lp_mint.key();
    pool.liquidity = 0;
    pool.total_collateral = 0;
    pool.program_fees = 0;
    pool.insurance_fund = 0;
    pool.long_size = 0;
    pool.short_size = 0;
    pool.long_size_scaled = 0;
    pool.short_size_scaled = 0;
    pool.cumulative_funding = 0;
    pool.last_funding_timestamp = current_timestamp;
    pool.average_price = initial_price;
    pool.last_oracle_price = initial_price;
    pool.average_price_timestamp = current_timestamp;
    pool.funding_rate_per_second = parameters.funding_rate_per_second;
    pool.open_fee_bps = parameters.open_fee_bps;
    pool.close_fee_bps = parameters.close_fee_bps;
    pool.initial_margin_bps = parameters.initial_margin_bps;
    pool.maintenance_margin_bps = parameters.maintenance_margin_bps;
    pool.liquidation_fee_bps = parameters.liquidation_fee_bps;
    pool.max_confidence_bps = parameters.max_confidence_bps;
    pool.max_price_deviation_bps = parameters.max_price_deviation_bps;
    pool.insurance_fee_bps = parameters.insurance_fee_bps;
    pool.profit_warmup_slots = parameters.profit_warmup_slots;
    pool.bump = context.bumps.pool;

    Ok(())
}

#[derive(Accounts)]
pub struct InitializePoolAccountConstraints<'info> {
    #[account(mut)]
    pub authority: Signer<'info>,

    #[account(
        init,
        payer = authority,
        space = Pool::DISCRIMINATOR.len() + Pool::INIT_SPACE,
        seeds = [POOL_SEED, collateral_mint.key().as_ref(), oracle_feed.key().as_ref()],
        bump,
    )]
    pub pool: Box<Account<'info, Pool>>,

    pub collateral_mint: Box<InterfaceAccount<'info, Mint>>,

    /// CHECK: The oracle feed account. Its key and its owning program are
    /// stored on the pool, and every read, including the one here that seeds
    /// the average price, requires that owner and validates the layout, scale,
    /// and freshness; it is never trusted by type. Swap for a real Pyth price
    /// feed in production.
    pub oracle_feed: UncheckedAccount<'info>,

    /// Liquidity-provider share mint. The pool account is its mint authority
    /// and signs every mint and burn with its own seeds.
    #[account(
        init,
        payer = authority,
        seeds = [LP_MINT_SEED, pool.key().as_ref()],
        bump,
        mint::decimals = collateral_mint.decimals,
        mint::authority = pool,
        mint::token_program = token_program,
    )]
    pub lp_mint: Box<InterfaceAccount<'info, Mint>>,

    /// Custody vault for all collateral. The pool account owns it and signs
    /// every transfer out with its own seeds.
    #[account(
        init,
        payer = authority,
        seeds = [VAULT_SEED, pool.key().as_ref()],
        bump,
        token::mint = collateral_mint,
        token::authority = pool,
        token::token_program = token_program,
    )]
    pub custody_vault: Box<InterfaceAccount<'info, TokenAccount>>,

    pub token_program: Interface<'info, TokenInterface>,
    pub associated_token_program: Program<'info, AssociatedToken>,
    pub system_program: Program<'info, System>,
}
