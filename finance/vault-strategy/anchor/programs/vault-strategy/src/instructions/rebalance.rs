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
pub struct RebalanceAccountConstraints {
    #[account(address = strategy.manager)]
    pub manager: Signer,

    #[account(
        mut,
        seeds = [b"strategy", strategy.index.to_le_bytes()],
        bump = strategy.bump,
    )]
    pub strategy: Box<BorshAccount<Strategy>>,

    #[account(address = strategy.usdc_mint @ VaultError::InvalidUsdcMint)]
    pub usdc_mint: Box<InterfaceAccount<Mint>>,

    #[account(mut)]
    pub sell_mint: Box<InterfaceAccount<Mint>>,

    #[account(mut)]
    pub buy_mint: Box<InterfaceAccount<Mint>>,

    #[account(
        constraint = sell_config.strategy == *strategy.address() @ VaultError::InvalidAssetAccount,
        constraint = sell_config.mint == *sell_mint.address() @ VaultError::AssetNotFound,
        constraint = sell_config.vault == *vault_sell.address() @ VaultError::InvalidAssetAccount,
    )]
    pub sell_config: Box<BorshAccount<AssetConfig>>,

    #[account(
        constraint = buy_config.strategy == *strategy.address() @ VaultError::InvalidAssetAccount,
        constraint = buy_config.mint == *buy_mint.address() @ VaultError::AssetNotFound,
        constraint = buy_config.vault == *vault_buy.address() @ VaultError::InvalidAssetAccount,
    )]
    pub buy_config: Box<BorshAccount<AssetConfig>>,

    /// CHECK: Pyth feed - validated against sell asset's registered feed
    #[account(constraint = *sell_price_feed.address() == sell_config.price_feed @ VaultError::InvalidPriceFeed)]
    pub sell_price_feed: UncheckedAccount,

    /// CHECK: Pyth feed - validated against buy asset's registered feed
    #[account(constraint = *buy_price_feed.address() == buy_config.price_feed @ VaultError::InvalidPriceFeed)]
    pub buy_price_feed: UncheckedAccount,

    #[account(
        mut,
        associated_token::mint = sell_mint,
        associated_token::authority = strategy,
        associated_token::token_program = token_program
    )]
    pub vault_sell: Box<InterfaceAccount<TokenAccount>>,

    #[account(
        mut,
        associated_token::mint = buy_mint,
        associated_token::authority = strategy,
        associated_token::token_program = token_program
    )]
    pub vault_buy: Box<InterfaceAccount<TokenAccount>>,

    #[account(
        mut,
        associated_token::mint = usdc_mint,
        associated_token::authority = strategy,
        associated_token::token_program = token_program
    )]
    pub vault_usdc: Box<InterfaceAccount<TokenAccount>>,

    pub sell_rate: BorshAccount<AssetRate>,

    pub buy_rate: BorshAccount<AssetRate>,

    /// CHECK: Router config PDA
    #[account(mut)]
    pub router_config: UncheckedAccount,

    /// CHECK: Router USDC treasury ATA
    #[account(mut)]
    pub router_usdc_treasury: UncheckedAccount,

    /// CHECK: Router authority PDA
    #[account(mut)]
    pub router_authority: UncheckedAccount,

    #[account(
        constraint = *swap_router_program.address() == strategy.swap_router @ VaultError::InvalidSwapRouter
    )]
    /// CHECK: validated by the address constraint above
    pub swap_router_program: UncheckedAccount,

    pub associated_token_program: Program<AssociatedToken>,
    pub token_program: Interface<'static, TokenInterface>,
    pub system_program: Program<System>,
}

pub fn handle_rebalance(
    context: &mut Context<RebalanceAccountConstraints>,
    sell_amount: u64,
    usdc_to_invest: u64,
) -> Result<()> {
    require!(
        context.accounts.sell_mint.address() != context.accounts.buy_mint.address(),
        VaultError::SameMint
    );

    let strategy = &context.accounts.strategy;
    let strategy_index = strategy.index;
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

    let index_bytes = strategy_index.to_le_bytes();
    let signer_seeds: &[&[&[u8]]] = &[&[b"strategy", index_bytes.as_ref(), &[strategy_bump]]];

    // Step 1: sell basket token -> USDC
    let sell_cpi_accounts = RouterSellAccounts {
        caller: context.accounts.strategy.cpi_handle(),
        router_config: context.accounts.router_config.cpi_handle(),
        asset_rate: context.accounts.sell_rate.cpi_handle(),
        usdc_mint: context.accounts.usdc_mint.cpi_handle(),
        asset_mint: context.accounts.sell_mint.cpi_handle_mut(),
        caller_asset_account: context.accounts.vault_sell.cpi_handle_mut(),
        caller_usdc_account: context.accounts.vault_usdc.cpi_handle_mut(),
        router_usdc_treasury: context.accounts.router_usdc_treasury.cpi_handle_mut(),
        router_authority: context.accounts.router_authority.cpi_handle(),
        associated_token_program: context.accounts.associated_token_program.cpi_handle(),
        token_program: context.accounts.token_program.cpi_handle(),
        system_program: context.accounts.system_program.cpi_handle(),
    };
    mock_swap_router::cpi::swap_asset_for_usdc(
        CpiContext::new_with_signer(
            context.accounts.swap_router_program.address(),
            sell_cpi_accounts,
            signer_seeds,
        ),
        sell_amount,
        minimum_usdc_from_sell,
    )?;

    // Step 2: buy basket token with USDC
    let buy_cpi_accounts = RouterBuyAccounts {
        caller: context.accounts.strategy.cpi_handle(),
        router_config: context.accounts.router_config.cpi_handle(),
        asset_rate: context.accounts.buy_rate.cpi_handle(),
        usdc_mint: context.accounts.usdc_mint.cpi_handle(),
        asset_mint: context.accounts.buy_mint.cpi_handle_mut(),
        caller_usdc_account: context.accounts.vault_usdc.cpi_handle_mut(),
        caller_asset_account: context.accounts.vault_buy.cpi_handle_mut(),
        router_usdc_treasury: context.accounts.router_usdc_treasury.cpi_handle_mut(),
        router_authority: context.accounts.router_authority.cpi_handle(),
        associated_token_program: context.accounts.associated_token_program.cpi_handle(),
        token_program: context.accounts.token_program.cpi_handle(),
        system_program: context.accounts.system_program.cpi_handle(),
    };
    mock_swap_router::cpi::swap_usdc_for_asset(
        CpiContext::new_with_signer(
            context.accounts.swap_router_program.address(),
            buy_cpi_accounts,
            signer_seeds,
        ),
        usdc_to_invest,
        minimum_buy_amount,
    )?;

    Ok(())
}
