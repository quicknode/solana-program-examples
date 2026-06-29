use anchor_lang::prelude::*;
use anchor_spl::{
    associated_token::AssociatedToken,
    token_interface::{Mint, TokenAccount, TokenInterface},
};
use mock_swap_router::{
    cpi::accounts::SwapUsdcForAssetAccountConstraints as RouterSwapAccounts, state::AssetRate,
};

use crate::error::VaultError;
use crate::oracle::{load_price, PYTH_PRICE_PRECISION};
use crate::state::{AssetConfig, Strategy};

#[derive(Accounts)]
pub struct InvestAccountConstraints<'info> {
    /// Only the manager may call invest
    pub manager: Signer<'info>,

    #[account(
        mut,
        has_one = manager,
        has_one = usdc_mint @ VaultError::InvalidUsdcMint,
        seeds = [b"strategy", strategy.index.to_le_bytes().as_ref()],
        bump = strategy.bump
    )]
    pub strategy: Box<Account<'info, Strategy>>,

    /// The asset to buy. Validated against its registered config below.
    #[account(
        constraint = asset_config.strategy == strategy.key() @ VaultError::InvalidAssetAccount,
        constraint = asset_config.mint == asset_mint.key() @ VaultError::AssetNotFound,
        constraint = asset_config.vault == vault_asset.key() @ VaultError::InvalidAssetAccount,
    )]
    pub asset_config: Box<Account<'info, AssetConfig>>,

    pub usdc_mint: Box<InterfaceAccount<'info, Mint>>,

    #[account(mut)]
    pub asset_mint: Box<InterfaceAccount<'info, Mint>>,

    /// CHECK: Pyth feed - validated against the asset's registered feed
    #[account(
        constraint = price_feed.key() == asset_config.price_feed @ VaultError::InvalidPriceFeed
    )]
    pub price_feed: UncheckedAccount<'info>,

    #[account(
        mut,
        associated_token::mint = usdc_mint,
        associated_token::authority = strategy,
        associated_token::token_program = token_program
    )]
    pub vault_usdc: Box<InterfaceAccount<'info, TokenAccount>>,

    #[account(
        mut,
        associated_token::mint = asset_mint,
        associated_token::authority = strategy,
        associated_token::token_program = token_program
    )]
    pub vault_asset: Box<InterfaceAccount<'info, TokenAccount>>,

    pub asset_rate: Account<'info, AssetRate>,

    /// CHECK: Router config PDA from the mock-swap-router program
    #[account(mut)]
    pub router_config: UncheckedAccount<'info>,

    /// CHECK: Router USDC treasury ATA
    #[account(mut)]
    pub router_usdc_treasury: UncheckedAccount<'info>,

    /// CHECK: Router authority PDA from the mock-swap-router program
    #[account(mut)]
    pub router_authority: UncheckedAccount<'info>,

    #[account(
        constraint = swap_router_program.key() == strategy.swap_router @ VaultError::InvalidSwapRouter
    )]
    pub swap_router_program: Program<'info, mock_swap_router::program::MockSwapRouter>,

    pub associated_token_program: Program<'info, AssociatedToken>,
    pub token_program: Interface<'info, TokenInterface>,
    pub system_program: Program<'info, System>,
}

pub fn handle_invest(context: Context<InvestAccountConstraints>, usdc_amount: u64) -> Result<()> {
    let strategy = &context.accounts.strategy;
    let strategy_index = strategy.index;
    let strategy_bump = strategy.bump;
    let max_slippage_bps = strategy.max_slippage_bps;

    // Slippage floor anchored to the oracle, not to a manager-supplied number:
    // expected_out = usdc_amount * 10^8 / price, then allow it to fall short by at
    // most max_slippage_bps. The router rejects any fill below this.
    let now = Clock::get()?.unix_timestamp;
    let price = load_price(
        &context.accounts.price_feed,
        &context.accounts.asset_config.price_feed,
        now,
    )?;

    let expected_out = (usdc_amount as u128)
        .checked_mul(PYTH_PRICE_PRECISION)
        .ok_or(VaultError::MathOverflow)?
        .checked_div(price)
        .ok_or(VaultError::MathOverflow)?;
    let minimum_asset_out: u64 = expected_out
        .checked_mul((10_000 - max_slippage_bps) as u128)
        .ok_or(VaultError::MathOverflow)?
        .checked_div(10_000)
        .ok_or(VaultError::MathOverflow)?
        .try_into()
        .map_err(|_| VaultError::MathOverflow)?;

    let index_bytes = strategy_index.to_le_bytes();
    let signer_seeds: &[&[&[u8]]] = &[&[b"strategy", index_bytes.as_ref(), &[strategy_bump]]];

    let cpi_accounts = RouterSwapAccounts {
        caller: context.accounts.strategy.to_account_info(),
        router_config: context.accounts.router_config.to_account_info(),
        asset_rate: context.accounts.asset_rate.to_account_info(),
        usdc_mint: context.accounts.usdc_mint.to_account_info(),
        asset_mint: context.accounts.asset_mint.to_account_info(),
        caller_usdc_account: context.accounts.vault_usdc.to_account_info(),
        caller_asset_account: context.accounts.vault_asset.to_account_info(),
        router_usdc_treasury: context.accounts.router_usdc_treasury.to_account_info(),
        router_authority: context.accounts.router_authority.to_account_info(),
        associated_token_program: context.accounts.associated_token_program.to_account_info(),
        token_program: context.accounts.token_program.to_account_info(),
        system_program: context.accounts.system_program.to_account_info(),
    };

    let cpi_ctx = CpiContext::new_with_signer(
        context.accounts.swap_router_program.key(),
        cpi_accounts,
        signer_seeds,
    );

    mock_swap_router::cpi::swap_usdc_for_asset(cpi_ctx, usdc_amount, minimum_asset_out)?;

    Ok(())
}
