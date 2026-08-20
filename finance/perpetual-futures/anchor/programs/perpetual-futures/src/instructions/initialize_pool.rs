use anchor_lang::prelude::*;
use anchor_spl::mint;
use anchor_spl::token;
use anchor_spl::{
    associated_token::AssociatedToken,
    token_interface::{Mint, TokenAccount, TokenInterface},
};

use crate::constants::{
    AUTHORITY_SEED, BASIS_POINTS_DENOMINATOR, LP_MINT_SEED, MAX_LEVERAGE_CEILING, POOL_SEED,
    VAULT_SEED,
};
use crate::errors::PerpError;
use crate::state::Pool;

/// Trading parameters set once at pool creation. Bundled into one struct so the
/// instruction signature stays readable.
#[derive(Clone, IdlType, wincode::SchemaRead, wincode::SchemaWrite)]
pub struct PoolParameters {
    /// Decimal places the oracle quotes its price in (e.g. 8).
    pub oracle_scale: u32,

    /// Funding accrued per slot, in `FUNDING_PRECISION` units, charged to the
    /// heavier side.
    pub funding_rate_per_slot: u64,

    pub open_fee_bps: u16,
    pub close_fee_bps: u16,
    pub max_leverage: u16,
    pub maintenance_margin_bps: u16,
    pub liquidation_fee_bps: u16,

    /// Maximum oracle confidence band tolerated, in basis points of the price.
    pub max_confidence_bps: u16,
}

pub fn handle_initialize_pool(
    context: &mut Context<InitializePoolAccountConstraints>,
    parameters: PoolParameters,
) -> Result<()> {
    let denominator = BASIS_POINTS_DENOMINATOR as u16;
    require!(
        parameters.max_leverage >= 1 && parameters.max_leverage <= MAX_LEVERAGE_CEILING,
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
    // Zero would reject every real feed (which always reports some uncertainty);
    // above 100% is meaningless. Anything in between is a valid risk choice.
    require!(
        parameters.max_confidence_bps > 0 && parameters.max_confidence_bps < denominator,
        PerpError::InvalidParameter
    );

    let pool = &mut context.accounts.pool;
    pool.authority = *context.accounts.authority.address();
    pool.collateral_mint = *context.accounts.collateral_mint.address();
    pool.oracle_feed = *context.accounts.oracle_feed.address();
    pool.oracle_scale = parameters.oracle_scale;
    pool.custody_vault = *context.accounts.custody_vault.address();
    pool.lp_mint = *context.accounts.lp_mint.address();
    pool.liquidity = 0;
    pool.reserved_liquidity = 0;
    pool.total_collateral = 0;
    pool.protocol_fees = 0;
    pool.long_size = 0;
    pool.short_size = 0;
    pool.long_size_scaled = 0;
    pool.short_size_scaled = 0;
    pool.cumulative_funding = 0;
    pool.last_funding_slot = Clock::get()?.slot;
    pool.funding_rate_per_slot = parameters.funding_rate_per_slot;
    pool.open_fee_bps = parameters.open_fee_bps;
    pool.close_fee_bps = parameters.close_fee_bps;
    pool.max_leverage = parameters.max_leverage;
    pool.maintenance_margin_bps = parameters.maintenance_margin_bps;
    pool.liquidation_fee_bps = parameters.liquidation_fee_bps;
    pool.max_confidence_bps = parameters.max_confidence_bps;
    pool.bump = context.bumps.pool;
    pool.authority_bump = context.bumps.pool_authority;

    Ok(())
}

#[derive(Accounts)]
pub struct InitializePoolAccountConstraints {
    #[account(mut)]
    pub authority: Signer,

    #[account(
        init,
        payer = authority,
        space = Pool::DISCRIMINATOR.len() + Pool::INIT_SPACE,
        seeds = [POOL_SEED, collateral_mint.address().as_ref(), oracle_feed.address().as_ref()],
        bump,
    )]
    pub pool: Box<BorshAccount<Pool>>,

    pub collateral_mint: Box<InterfaceAccount<Mint>>,

    /// CHECK: The oracle feed account. Its key is stored on the pool and every
    /// read validates the layout, scale, and freshness; it is never trusted by
    /// type. Swap for a real Switchboard feed in production.
    pub oracle_feed: UncheckedAccount,

    /// CHECK: PDA that owns the vault and the liquidity-provider mint. Holds no
    /// data; used only to sign vault and mint CPIs.
    #[account(
        seeds = [AUTHORITY_SEED, pool.address().as_ref()],
        bump,
    )]
    pub pool_authority: UncheckedAccount,

    #[account(
        init,
        payer = authority,
        seeds = [LP_MINT_SEED, pool.address().as_ref()],
        bump,
        mint::decimals = collateral_mint.decimals(),
        mint::authority = pool_authority,
        mint::token_program = token_program,
    )]
    pub lp_mint: Box<InterfaceAccount<Mint>>,

    #[account(
        init,
        payer = authority,
        seeds = [VAULT_SEED, pool.address().as_ref()],
        bump,
        token::mint = collateral_mint,
        token::authority = pool_authority,
        token::token_program = token_program,
    )]
    pub custody_vault: Box<InterfaceAccount<TokenAccount>>,

    pub token_program: Interface<'static, TokenInterface>,
    pub associated_token_program: Program<AssociatedToken>,
    pub system_program: Program<System>,
}
