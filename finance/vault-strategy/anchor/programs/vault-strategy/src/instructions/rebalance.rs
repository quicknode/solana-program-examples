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
use crate::oracle::{load_price, PYTH_PRICE_PRECISION};
use crate::state::{AssetConfig, Strategy};

#[derive(Accounts)]
pub struct RebalanceAccountConstraints<'info> {
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

    #[account(mut)]
    pub sell_mint: Box<InterfaceAccount<'info, Mint>>,

    #[account(mut)]
    pub buy_mint: Box<InterfaceAccount<'info, Mint>>,

    #[account(
        constraint = sell_config.strategy == strategy.key() @ VaultError::InvalidAssetAccount,
        constraint = sell_config.mint == sell_mint.key() @ VaultError::AssetNotFound,
        constraint = sell_config.vault == vault_sell.key() @ VaultError::InvalidAssetAccount,
    )]
    pub sell_config: Box<Account<'info, AssetConfig>>,

    #[account(
        constraint = buy_config.strategy == strategy.key() @ VaultError::InvalidAssetAccount,
        constraint = buy_config.mint == buy_mint.key() @ VaultError::AssetNotFound,
        constraint = buy_config.vault == vault_buy.key() @ VaultError::InvalidAssetAccount,
    )]
    pub buy_config: Box<Account<'info, AssetConfig>>,

    /// CHECK: Pyth feed - validated against sell asset's registered feed
    #[account(constraint = sell_price_feed.key() == sell_config.price_feed @ VaultError::InvalidPriceFeed)]
    pub sell_price_feed: UncheckedAccount<'info>,

    /// CHECK: Pyth feed - validated against buy asset's registered feed
    #[account(constraint = buy_price_feed.key() == buy_config.price_feed @ VaultError::InvalidPriceFeed)]
    pub buy_price_feed: UncheckedAccount<'info>,

    #[account(
        mut,
        associated_token::mint = sell_mint,
        associated_token::authority = strategy,
        associated_token::token_program = token_program
    )]
    pub vault_sell: Box<InterfaceAccount<'info, TokenAccount>>,

    #[account(
        mut,
        associated_token::mint = buy_mint,
        associated_token::authority = strategy,
        associated_token::token_program = token_program
    )]
    pub vault_buy: Box<InterfaceAccount<'info, TokenAccount>>,

    #[account(
        mut,
        associated_token::mint = usdc_mint,
        associated_token::authority = strategy,
        associated_token::token_program = token_program
    )]
    pub vault_usdc: Box<InterfaceAccount<'info, TokenAccount>>,

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

    #[account(
        constraint = swap_router_program.key() == strategy.swap_router @ VaultError::InvalidSwapRouter
    )]
    pub swap_router_program: Program<'info, mock_swap_router::program::MockSwapRouter>,

    pub associated_token_program: Program<'info, AssociatedToken>,
    pub token_program: Interface<'info, TokenInterface>,
    pub system_program: Program<'info, System>,
}

pub fn handle_rebalance(
    context: Context<RebalanceAccountConstraints>,
    sell_amount: u64,
    usdc_to_invest: u64,
) -> Result<()> {
    require!(
        context.accounts.sell_mint.key() != context.accounts.buy_mint.key(),
        VaultError::SameMint
    );

    let strategy = &context.accounts.strategy;
    let manager_key = strategy.manager;
    let strategy_bump = strategy.bump;
    let slip = (10_000 - strategy.max_slippage_bps) as u128;

    let now = Clock::get()?.unix_timestamp;
    let price_sell = load_price(
        &context.accounts.sell_price_feed,
        &context.accounts.sell_config.price_feed,
        now,
    )?;
    let price_buy = load_price(
        &context.accounts.buy_price_feed,
        &context.accounts.buy_config.price_feed,
        now,
    )?;

    // Sell leg floor: USDC out must be within slippage of the oracle value of what we sell.
    let expected_usdc = (sell_amount as u128)
        .checked_mul(price_sell)
        .ok_or(VaultError::MathOverflow)?
        .checked_div(PYTH_PRICE_PRECISION)
        .ok_or(VaultError::MathOverflow)?;
    let minimum_usdc_from_sell: u64 = expected_usdc
        .checked_mul(slip)
        .ok_or(VaultError::MathOverflow)?
        .checked_div(10_000)
        .ok_or(VaultError::MathOverflow)?
        .try_into()
        .map_err(|_| VaultError::MathOverflow)?;

    // Buy leg floor: asset out must be within slippage of the oracle-implied amount.
    let expected_buy = (usdc_to_invest as u128)
        .checked_mul(PYTH_PRICE_PRECISION)
        .ok_or(VaultError::MathOverflow)?
        .checked_div(price_buy)
        .ok_or(VaultError::MathOverflow)?;
    let minimum_buy_amount: u64 = expected_buy
        .checked_mul(slip)
        .ok_or(VaultError::MathOverflow)?
        .checked_div(10_000)
        .ok_or(VaultError::MathOverflow)?
        .try_into()
        .map_err(|_| VaultError::MathOverflow)?;

    let signer_seeds: &[&[&[u8]]] = &[&[b"strategy", manager_key.as_ref(), &[strategy_bump]]];

    // Step 1: sell basket token -> USDC
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
