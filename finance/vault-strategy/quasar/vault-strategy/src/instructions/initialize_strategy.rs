use quasar_lang::prelude::*;
use quasar_lang::sysvars::Sysvar as _;
use quasar_spl::prelude::*;

use crate::errors::VaultError;
use crate::state::{Registry, ShareMintPda, Strategy, StrategyInner, UsdcVaultPda};

/// Highest annual management fee a manager may set (10%). `collect_fees` mints
/// shares to the manager and dilutes every depositor, so an uncapped fee would
/// let a manager drain the vault by configuration.
pub const MAX_FEE_BPS: u16 = 1_000;

/// Highest slippage tolerance a manager may set (10%). Caps how loose the
/// oracle-anchored swap bound in deposit/rebalance can be made.
pub const MAX_SLIPPAGE_BPS: u16 = 1_000;

#[derive(Accounts)]
#[instruction(index: u64)]
pub struct InitializeStrategyAccountConstraints {
    #[account(mut)]
    pub manager: Signer,

    pub usdc_mint: Account<Mint>,

    pub registry: Account<Registry>,

    #[account(init, payer = manager, address = Strategy::seeds(index))]
    pub strategy: Account<Strategy>,

    #[account(
        init,
        payer = manager,
        mint(decimals = 6, authority = strategy, freeze_authority = None, token_program = token_program),
        address = ShareMintPda::seeds(strategy.address()),
    )]
    pub share_mint: Account<Mint>,

    #[account(
        init,
        payer = manager,
        token(mint = usdc_mint, authority = strategy, token_program = token_program),
        address = UsdcVaultPda::seeds(strategy.address()),
    )]
    pub vault_usdc: InterfaceAccount<Token>,

    pub rent: Sysvar<Rent>,
    pub token_program: Program<TokenProgram>,
    pub system_program: Program<SystemProgram>,
}

#[inline(always)]
pub fn handle_initialize_strategy(
    accounts: &mut InitializeStrategyAccountConstraints,
    index: u64,
    fee_bps: u16,
    max_slippage_bps: u16,
    swap_router: Address,
    bumps: &InitializeStrategyAccountConstraintsBumps,
) -> Result<(), ProgramError> {
    require!(fee_bps <= MAX_FEE_BPS, VaultError::FeeTooHigh);
    require!(
        max_slippage_bps <= MAX_SLIPPAGE_BPS,
        VaultError::SlippageConfigTooHigh
    );

    let now = i64::from(Clock::get()?.unix_timestamp);

    accounts.strategy.set_inner(StrategyInner {
        index,
        manager: *accounts.manager.address(),
        registry: *accounts.registry.address(),
        share_mint: *accounts.share_mint.address(),
        usdc_mint: *accounts.usdc_mint.address(),
        swap_router,
        fee_bps,
        max_slippage_bps,
        total_shares: 0,
        last_fee_accrual_timestamp: now,
        asset_count: 0,
        total_weight_bps: 0,
        bump: bumps.strategy,
    });
    Ok(())
}
