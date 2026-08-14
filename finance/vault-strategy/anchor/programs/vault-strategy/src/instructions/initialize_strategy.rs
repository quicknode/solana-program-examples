use anchor_lang::prelude::*;
use anchor_spl::mint;
use anchor_spl::{
    associated_token::AssociatedToken,
    token_interface::{Mint, TokenAccount, TokenInterface},
};

use crate::error::VaultError;
use crate::state::{Registry, Strategy};

/// Highest annual management fee a manager may set, in basis points (10%).
/// `collect_fees` mints shares to the manager and dilutes every depositor,
/// so an uncapped fee would let a manager drain the vault by configuration;
/// 10% per year is already far above typical fund management fees.
pub const MAX_FEE_BPS: u16 = 1_000;

/// Highest slippage tolerance a manager may set, in basis points (10%).
/// deposit/rebalance reject a swap whose output deviates from the Pyth price by
/// more than this; capping it stops a manager from setting a tolerance so loose
/// that the bound is meaningless.
pub const MAX_SLIPPAGE_BPS: u16 = 1_000;

#[derive(Accounts)]
#[instruction(index: u64)]
pub struct InitializeStrategyAccountConstraints {
    #[account(mut)]
    pub manager: Signer,

    pub usdc_mint: InterfaceAccount<Mint>,

    /// Registry whose approved assets this strategy may hold.
    pub registry: BorshAccount<Registry>,

    #[account(
        init,
        payer = manager,
        space = Strategy::DISCRIMINATOR.len() + Strategy::INIT_SPACE,
        seeds = [b"strategy", index.to_le_bytes()],
        bump
    )]
    pub strategy: Box<BorshAccount<Strategy>>,

    #[account(
        init,
        payer = manager,
        mint::decimals = 6,
        mint::authority = strategy,
        mint::freeze_authority = strategy,
        mint::token_program = token_program,
        seeds = [b"share_mint", strategy.address().as_ref()],
        bump
    )]
    pub share_mint: Box<InterfaceAccount<Mint>>,

    /// Vault's USDC token account - strategy PDA is the authority
    #[account(
        init,
        payer = manager,
        associated_token::mint = usdc_mint,
        associated_token::authority = strategy,
        associated_token::token_program = token_program
    )]
    pub vault_usdc: Box<InterfaceAccount<TokenAccount>>,

    pub associated_token_program: Program<AssociatedToken>,
    pub token_program: Interface<'static, TokenInterface>,
    pub system_program: Program<System>,
}

pub fn handle_initialize_strategy(
    context: &mut Context<InitializeStrategyAccountConstraints>,
    index: u64,
    fee_bps: u16,
    max_slippage_bps: u16,
    swap_router: Address,
) -> Result<()> {
    require!(fee_bps <= MAX_FEE_BPS, VaultError::FeeTooHigh);
    require!(
        max_slippage_bps <= MAX_SLIPPAGE_BPS,
        VaultError::SlippageConfigTooHigh
    );

    let clock = Clock::get()?;

    *context.accounts.strategy = (Strategy {
        index,
        manager: context.accounts.manager.address(),
        registry: context.accounts.registry.address(),
        share_mint: context.accounts.share_mint.address(),
        usdc_mint: context.accounts.usdc_mint.address(),
        swap_router,
        fee_bps,
        max_slippage_bps,
        total_shares: 0,
        last_fee_accrual_timestamp: clock.unix_timestamp,
        asset_count: 0,
        total_weight_bps: 0,
        bump: context.bumps.strategy,
    });

    Ok(())
}
