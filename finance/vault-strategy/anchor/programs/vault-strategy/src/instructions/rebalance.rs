use anchor_lang::prelude::*;
use anchor_spl::{
    associated_token::AssociatedToken,
    token_interface::{Mint, TokenAccount, TokenInterface},
};
use mock_swap_router::{
    cpi::accounts::SwapAssetForUsdcAccountConstraints as RouterSellAccounts,
    cpi::accounts::SwapUsdcForAssetAccountConstraints as RouterBuyAccounts, state::AssetRate,
};

use crate::error::VaultError;
use crate::state::Strategy;

#[derive(Accounts)]
pub struct RebalanceAccountConstraints<'info> {
    pub manager: Signer<'info>,

    #[account(
        mut,
        has_one = manager,
        seeds = [b"strategy", strategy.manager.as_ref()],
        bump = strategy.bump
    )]
    pub strategy: Account<'info, Strategy>,

    pub usdc_mint: InterfaceAccount<'info, Mint>,

    /// The basket token being sold
    #[account(mut)]
    pub sell_mint: InterfaceAccount<'info, Mint>,

    /// The basket token being bought
    #[account(mut)]
    pub buy_mint: InterfaceAccount<'info, Mint>,

    /// Vault's token account for the asset being sold
    #[account(
        mut,
        associated_token::mint = sell_mint,
        associated_token::authority = strategy,
        associated_token::token_program = token_program
    )]
    pub vault_sell: InterfaceAccount<'info, TokenAccount>,

    /// Vault's token account for the asset being bought
    #[account(
        mut,
        associated_token::mint = buy_mint,
        associated_token::authority = strategy,
        associated_token::token_program = token_program
    )]
    pub vault_buy: InterfaceAccount<'info, TokenAccount>,

    #[account(
        mut,
        associated_token::mint = usdc_mint,
        associated_token::authority = strategy,
        associated_token::token_program = token_program
    )]
    pub vault_usdc: InterfaceAccount<'info, TokenAccount>,

    pub sell_rate: Account<'info, AssetRate>,

    pub buy_rate: Account<'info, AssetRate>,

    /// CHECK: Router config PDA
    #[account(mut)]
    pub router_config: UncheckedAccount<'info>,

    /// CHECK: Router USDC treasury ATA
    #[account(mut)]
    pub router_usdc_treasury: UncheckedAccount<'info>,

    /// CHECK: Router authority PDA
    #[account(mut)]
    pub router_authority: UncheckedAccount<'info>,

    pub swap_router_program: Program<'info, mock_swap_router::program::MockSwapRouter>,

    pub associated_token_program: Program<'info, AssociatedToken>,
    pub token_program: Interface<'info, TokenInterface>,
    pub system_program: Program<'info, System>,
}

pub fn handle_rebalance(
    context: Context<RebalanceAccountConstraints>,
    sell_amount: u64,
    minimum_usdc_from_sell: u64,
    usdc_to_invest: u64,
    minimum_buy_amount: u64,
) -> Result<()> {
    let strategy = &context.accounts.strategy;

    // Both sell and buy mints must be registered basket assets
    require!(
        context.accounts.sell_mint.key() == strategy.asset_mint_a
            || context.accounts.sell_mint.key() == strategy.asset_mint_b,
        VaultError::InvalidAssetMint
    );
    require!(
        context.accounts.buy_mint.key() == strategy.asset_mint_a
            || context.accounts.buy_mint.key() == strategy.asset_mint_b,
        VaultError::InvalidAssetMint
    );
    require!(
        context.accounts.sell_mint.key() != context.accounts.buy_mint.key(),
        VaultError::SameMint
    );

    let manager_key = strategy.manager;
    let strategy_bump = strategy.bump;
    let signer_seeds: &[&[&[u8]]] = &[&[b"strategy", manager_key.as_ref(), &[strategy_bump]]];

    // Step 1: sell basket token → USDC
    let sell_cpi_accounts = RouterSellAccounts {
        caller: context.accounts.strategy.to_account_info(),
        router_config: context.accounts.router_config.to_account_info(),
        asset_rate: context.accounts.sell_rate.to_account_info(),
        usdc_mint: context.accounts.usdc_mint.to_account_info(),
        asset_mint: context.accounts.sell_mint.to_account_info(),
        caller_asset_account: context.accounts.vault_sell.to_account_info(),
        caller_usdc_account: context.accounts.vault_usdc.to_account_info(),
        router_usdc_treasury: context.accounts.router_usdc_treasury.to_account_info(),
        router_authority: context.accounts.router_authority.to_account_info(),
        associated_token_program: context.accounts.associated_token_program.to_account_info(),
        token_program: context.accounts.token_program.to_account_info(),
        system_program: context.accounts.system_program.to_account_info(),
    };
    mock_swap_router::cpi::swap_asset_for_usdc(
        CpiContext::new_with_signer(
            context.accounts.swap_router_program.key(),
            sell_cpi_accounts,
            signer_seeds,
        ),
        sell_amount,
        minimum_usdc_from_sell,
    )?;

    // Step 2: buy basket token with USDC
    let buy_cpi_accounts = RouterBuyAccounts {
        caller: context.accounts.strategy.to_account_info(),
        router_config: context.accounts.router_config.to_account_info(),
        asset_rate: context.accounts.buy_rate.to_account_info(),
        usdc_mint: context.accounts.usdc_mint.to_account_info(),
        asset_mint: context.accounts.buy_mint.to_account_info(),
        caller_usdc_account: context.accounts.vault_usdc.to_account_info(),
        caller_asset_account: context.accounts.vault_buy.to_account_info(),
        router_usdc_treasury: context.accounts.router_usdc_treasury.to_account_info(),
        router_authority: context.accounts.router_authority.to_account_info(),
        associated_token_program: context.accounts.associated_token_program.to_account_info(),
        token_program: context.accounts.token_program.to_account_info(),
        system_program: context.accounts.system_program.to_account_info(),
    };
    mock_swap_router::cpi::swap_usdc_for_asset(
        CpiContext::new_with_signer(
            context.accounts.swap_router_program.key(),
            buy_cpi_accounts,
            signer_seeds,
        ),
        usdc_to_invest,
        minimum_buy_amount,
    )?;

    Ok(())
}
