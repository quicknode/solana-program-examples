use anchor_lang::prelude::*;
use anchor_spl::{
    associated_token::AssociatedToken,
    token_interface::{Mint, TokenAccount, TokenInterface},
};
use mock_swap_router::{
    cpi::accounts::SwapUsdcForAssetAccountConstraints as RouterSwapAccounts, state::AssetRate,
};

use crate::error::VaultError;
use crate::state::Strategy;

#[derive(Accounts)]
pub struct InvestAccountConstraints<'info> {
    /// Only the manager may call invest
    pub manager: Signer<'info>,

    #[account(
        mut,
        has_one = manager,
        has_one = usdc_mint @ VaultError::InvalidUsdcMint,
        seeds = [b"strategy", strategy.manager.as_ref()],
        bump = strategy.bump
    )]
    pub strategy: Box<Account<'info, Strategy>>,

    pub usdc_mint: Box<InterfaceAccount<'info, Mint>>,

    /// The asset mint to buy — must be asset_mint_a or asset_mint_b
    #[account(mut)]
    pub asset_mint: Box<InterfaceAccount<'info, Mint>>,

    #[account(
        mut,
        associated_token::mint = usdc_mint,
        associated_token::authority = strategy,
        associated_token::token_program = token_program
    )]
    pub vault_usdc: Box<InterfaceAccount<'info, TokenAccount>>,

    /// Vault's asset token account for the asset being bought
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

pub fn handle_invest(
    context: Context<InvestAccountConstraints>,
    usdc_amount: u64,
    minimum_asset_out: u64,
) -> Result<()> {
    let strategy = &context.accounts.strategy;

    // Validate asset mint is one of the two basket assets
    require!(
        context.accounts.asset_mint.key() == strategy.asset_mint_a
            || context.accounts.asset_mint.key() == strategy.asset_mint_b,
        VaultError::InvalidAssetMint
    );

    let manager_key = strategy.manager;
    let strategy_bump = strategy.bump;
    let signer_seeds: &[&[&[u8]]] = &[&[b"strategy", manager_key.as_ref(), &[strategy_bump]]];

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
