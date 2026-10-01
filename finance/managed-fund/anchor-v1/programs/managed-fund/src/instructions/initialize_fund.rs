use anchor_lang::prelude::*;
use anchor_spl::{
    associated_token::AssociatedToken,
    token_interface::{Mint, TokenAccount, TokenInterface},
};

use crate::error::FundError;
use crate::state::{Fund, Registry, MAX_ASSETS};

/// Highest annual management fee a manager may set, in basis points (10%).
/// `collect_fees` mints shares to the manager and dilutes every depositor,
/// so an uncapped fee would let a manager drain the fund by configuration;
/// 10% per year is already far above typical fund management fees.
pub const MAX_FEE_BPS: u16 = 1_000;

/// Highest slippage tolerance a manager may set, in basis points (10%).
/// deposit/rebalance reject a swap whose output deviates from the Pyth price by
/// more than this; capping it stops a manager from setting a tolerance so loose
/// that the bound is meaningless.
pub const MAX_SLIPPAGE_BPS: u16 = 1_000;

/// Lowest rebalance threshold a manager may set, in basis points of the fund's
/// value (one percentage point). Every rebalance pays slippage, so a threshold
/// near zero would let anyone trade the fund on every small price move.
pub const MIN_REBALANCE_THRESHOLD_BPS: u16 = 100;

/// Highest rebalance threshold a manager may set (twenty percentage points).
/// Past that, the target weights stop describing what the fund holds.
pub const MAX_REBALANCE_THRESHOLD_BPS: u16 = 2_000;

#[derive(Accounts)]
#[instruction(index: u64)]
pub struct InitializeFundAccountConstraints<'info> {
    #[account(mut)]
    pub manager: Signer<'info>,

    pub usdc_mint: InterfaceAccount<'info, Mint>,

    /// Registry whose approved assets this fund may hold.
    pub registry: Account<'info, Registry>,

    #[account(
        init,
        payer = manager,
        space = Fund::DISCRIMINATOR.len() + Fund::INIT_SPACE,
        seeds = [b"fund", index.to_le_bytes().as_ref()],
        bump
    )]
    pub fund: Box<Account<'info, Fund>>,

    #[account(
        init,
        payer = manager,
        mint::decimals = 6,
        mint::authority = fund,
        mint::freeze_authority = fund,
        mint::token_program = token_program,
        seeds = [b"share_mint", fund.key().as_ref()],
        bump
    )]
    pub share_mint: Box<InterfaceAccount<'info, Mint>>,

    /// Vault's USDC token account - fund PDA is the authority
    #[account(
        init,
        payer = manager,
        associated_token::mint = usdc_mint,
        associated_token::authority = fund,
        associated_token::token_program = token_program
    )]
    pub vault_usdc: Box<InterfaceAccount<'info, TokenAccount>>,

    pub associated_token_program: Program<'info, AssociatedToken>,
    pub token_program: Interface<'info, TokenInterface>,
    pub system_program: Program<'info, System>,
}

pub fn handle_initialize_fund(
    context: Context<InitializeFundAccountConstraints>,
    index: u64,
    fee_bps: u16,
    max_slippage_bps: u16,
    rebalance_threshold_bps: u16,
    swap_router: Pubkey,
) -> Result<()> {
    require!(fee_bps <= MAX_FEE_BPS, FundError::FeeTooHigh);
    require!(
        max_slippage_bps <= MAX_SLIPPAGE_BPS,
        FundError::SlippageConfigTooHigh
    );
    require!(
        (MIN_REBALANCE_THRESHOLD_BPS..=MAX_REBALANCE_THRESHOLD_BPS)
            .contains(&rebalance_threshold_bps),
        FundError::RebalanceThresholdOutOfRange
    );

    let clock = Clock::get()?;

    context.accounts.fund.set_inner(Fund {
        index,
        manager: context.accounts.manager.key(),
        registry: context.accounts.registry.key(),
        share_mint: context.accounts.share_mint.key(),
        usdc_mint: context.accounts.usdc_mint.key(),
        usdc_decimals: context.accounts.usdc_mint.decimals,
        swap_router,
        fee_bps,
        max_slippage_bps,
        rebalance_threshold_bps,
        total_shares: 0,
        usdc_holdings: 0,
        asset_holdings: [0; MAX_ASSETS as usize],
        last_fee_accrual_timestamp: clock.unix_timestamp,
        asset_count: 0,
        total_weight_bps: 0,
        bump: context.bumps.fund,
    });

    Ok(())
}
