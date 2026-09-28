use anchor_lang::prelude::*;
use anchor_spl::{
    associated_token::AssociatedToken,
    token_interface::{Mint, TokenAccount, TokenInterface},
};
use mock_swap_router::{
    cpi::accounts::SwapAssetForUsdcAccountConstraints as RouterSellAccounts,
    cpi::accounts::SwapUsdcForAssetAccountConstraints as RouterBuyAccounts, state::AssetRate,
};

use crate::error::FundError;
use crate::oracle::{load_price, PYTH_PRICE_PRECISION};
use crate::state::{AssetConfig, Fund};

#[derive(Accounts)]
pub struct RebalanceAccountConstraints {
    #[account(address = fund.manager)]
    pub manager: Signer,

    #[account(
        mut,
        seeds = [b"fund", fund.index.to_le_bytes()],
        bump = fund.bump,
    )]
    pub fund: Box<BorshAccount<Fund>>,

    #[account(address = fund.usdc_mint @ FundError::InvalidUsdcMint)]
    pub usdc_mint: Box<InterfaceAccount<Mint>>,

    #[account(mut)]
    pub sell_mint: Box<InterfaceAccount<Mint>>,

    #[account(mut)]
    pub buy_mint: Box<InterfaceAccount<Mint>>,

    #[account(
        constraint = sell_config.fund == *fund.address() @ FundError::InvalidAssetAccount,
        constraint = sell_config.mint == *sell_mint.address() @ FundError::AssetNotFound,
        constraint = sell_config.vault == *vault_sell.address() @ FundError::InvalidAssetAccount,
    )]
    pub sell_config: Box<BorshAccount<AssetConfig>>,

    #[account(
        constraint = buy_config.fund == *fund.address() @ FundError::InvalidAssetAccount,
        constraint = buy_config.mint == *buy_mint.address() @ FundError::AssetNotFound,
        constraint = buy_config.vault == *vault_buy.address() @ FundError::InvalidAssetAccount,
    )]
    pub buy_config: Box<BorshAccount<AssetConfig>>,

    /// CHECK: Pyth feed - validated against sell asset's registered feed
    #[account(constraint = *sell_price_feed.address() == sell_config.price_feed @ FundError::InvalidPriceFeed)]
    pub sell_price_feed: UncheckedAccount,

    /// CHECK: Pyth feed - validated against buy asset's registered feed
    #[account(constraint = *buy_price_feed.address() == buy_config.price_feed @ FundError::InvalidPriceFeed)]
    pub buy_price_feed: UncheckedAccount,

    #[account(
        mut,
        associated_token::mint = sell_mint,
        associated_token::authority = fund,
        associated_token::token_program = token_program
    )]
    pub vault_sell: Box<InterfaceAccount<TokenAccount>>,

    #[account(
        mut,
        associated_token::mint = buy_mint,
        associated_token::authority = fund,
        associated_token::token_program = token_program
    )]
    pub vault_buy: Box<InterfaceAccount<TokenAccount>>,

    #[account(
        mut,
        associated_token::mint = usdc_mint,
        associated_token::authority = fund,
        associated_token::token_program = token_program
    )]
    pub vault_usdc: Box<InterfaceAccount<TokenAccount>>,

    pub sell_rate: BorshAccount<AssetRate>,

    pub buy_rate: BorshAccount<AssetRate>,

    /// CHECK: Router config PDA
    #[account(mut)]
    pub router_config: UncheckedAccount,

    /// CHECK: Router USDC treasury ATA, owned by the router config account
    #[account(mut)]
    pub router_usdc_treasury: UncheckedAccount,

    #[account(
        constraint = *swap_router_program.address() == fund.swap_router @ FundError::InvalidSwapRouter
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
        FundError::SameMint
    );

    let fund = &context.accounts.fund;
    let fund_index = fund.index;
    let fund_bump = fund.bump;
    let slip = (10_000 - fund.max_slippage_bps) as u128;

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
        .ok_or(FundError::MathOverflow)?
        .checked_div(PYTH_PRICE_PRECISION)
        .ok_or(FundError::MathOverflow)?;
    let minimum_usdc_from_sell: u64 = expected_usdc
        .checked_mul(slip)
        .ok_or(FundError::MathOverflow)?
        .checked_div(10_000)
        .ok_or(FundError::MathOverflow)?
        .try_into()
        .map_err(|_| FundError::MathOverflow)?;

    // Buy leg floor: asset out must be within slippage of the oracle-implied amount.
    let expected_buy = (usdc_to_invest as u128)
        .checked_mul(PYTH_PRICE_PRECISION)
        .ok_or(FundError::MathOverflow)?
        .checked_div(price_buy)
        .ok_or(FundError::MathOverflow)?;
    let minimum_buy_amount: u64 = expected_buy
        .checked_mul(slip)
        .ok_or(FundError::MathOverflow)?
        .checked_div(10_000)
        .ok_or(FundError::MathOverflow)?
        .try_into()
        .map_err(|_| FundError::MathOverflow)?;

    // Rebalancing may only trade what the program has accounted for: tokens
    // donated into a vault are outside the fund, so they can be neither sold nor
    // spent. The legs below record what each swap actually moved.
    let sell_index = context.accounts.sell_config.index as usize;
    let buy_index = context.accounts.buy_config.index as usize;
    let mut usdc_holdings = context.accounts.fund.usdc_holdings;
    let mut asset_holdings = context.accounts.fund.asset_holdings;
    require!(
        sell_amount <= asset_holdings[sell_index],
        FundError::InsufficientHoldings
    );

    let index_bytes = fund_index.to_le_bytes();
    let signer_seeds: &[&[&[u8]]] = &[&[b"fund", index_bytes.as_ref(), &[fund_bump]]];

    // `fund` signs the CPI(s) below. It is a data account holding a live
    // borrow on its buffer, which the runtime would reject when the CPI borrows
    // the same account, so hand the borrow back for the duration.
    context.accounts.fund.release_borrow()?;

    let sell_before = context.accounts.vault_sell.amount();
    let usdc_before_sell = context.accounts.vault_usdc.amount();

    // Step 1: sell basket token -> USDC
    let sell_cpi_accounts = RouterSellAccounts {
        caller: context.accounts.fund.to_cpi_handle(),
        router_config: context.accounts.router_config.cpi_handle(),
        asset_rate: context.accounts.sell_rate.cpi_handle(),
        usdc_mint: context.accounts.usdc_mint.to_cpi_handle(),
        asset_mint: context.accounts.sell_mint.to_cpi_handle_mut(),
        caller_asset_account: context.accounts.vault_sell.to_cpi_handle_mut(),
        caller_usdc_account: context.accounts.vault_usdc.to_cpi_handle_mut(),
        router_usdc_treasury: context.accounts.router_usdc_treasury.cpi_handle_mut(),
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

    let sold = sell_before
        .checked_sub(context.accounts.vault_sell.amount())
        .ok_or(FundError::MathOverflow)?;
    let usdc_after_sell = context.accounts.vault_usdc.amount();
    let usdc_received = usdc_after_sell
        .checked_sub(usdc_before_sell)
        .ok_or(FundError::MathOverflow)?;
    asset_holdings[sell_index] = asset_holdings[sell_index]
        .checked_sub(sold)
        .ok_or(FundError::InsufficientHoldings)?;
    usdc_holdings = usdc_holdings
        .checked_add(usdc_received)
        .ok_or(FundError::MathOverflow)?;
    require!(
        usdc_to_invest <= usdc_holdings,
        FundError::InsufficientHoldings
    );
    let buy_before = context.accounts.vault_buy.amount();

    // Step 2: buy basket token with USDC
    let buy_cpi_accounts = RouterBuyAccounts {
        caller: context.accounts.fund.to_cpi_handle(),
        router_config: context.accounts.router_config.cpi_handle(),
        asset_rate: context.accounts.buy_rate.cpi_handle(),
        usdc_mint: context.accounts.usdc_mint.to_cpi_handle(),
        asset_mint: context.accounts.buy_mint.to_cpi_handle_mut(),
        caller_usdc_account: context.accounts.vault_usdc.to_cpi_handle_mut(),
        caller_asset_account: context.accounts.vault_buy.to_cpi_handle_mut(),
        router_usdc_treasury: context.accounts.router_usdc_treasury.cpi_handle_mut(),
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

    let bought = context
        .accounts
        .vault_buy
        .amount()
        .checked_sub(buy_before)
        .ok_or(FundError::MathOverflow)?;
    let usdc_spent = usdc_after_sell
        .checked_sub(context.accounts.vault_usdc.amount())
        .ok_or(FundError::MathOverflow)?;
    asset_holdings[buy_index] = asset_holdings[buy_index]
        .checked_add(bought)
        .ok_or(FundError::MathOverflow)?;
    usdc_holdings = usdc_holdings
        .checked_sub(usdc_spent)
        .ok_or(FundError::InsufficientHoldings)?;

    context.accounts.fund.reacquire_borrow_mut()?;
    context.accounts.fund.usdc_holdings = usdc_holdings;
    context.accounts.fund.asset_holdings = asset_holdings;

    Ok(())
}
