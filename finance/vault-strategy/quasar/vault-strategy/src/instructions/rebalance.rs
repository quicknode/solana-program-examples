use quasar_lang::prelude::*;
use quasar_lang::sysvars::Sysvar as _;
use quasar_spl::prelude::*;

use crate::errors::VaultError;
use crate::oracle::{load_price, PYTH_PRICE_PRECISION};
use crate::state::{AssetConfig, Strategy, STRATEGY_SEED};

const ROUTER_SWAP_USDC_FOR_ASSET: u8 = 2;
const ROUTER_SWAP_ASSET_FOR_USDC: u8 = 3;
const SWAP_ACCOUNTS: usize = 10;
const SWAP_DATA_LEN: usize = 17;

#[derive(Accounts)]
pub struct RebalanceAccountConstraints {
    pub manager: Signer,

    #[account(
        mut,
        address = Strategy::seeds(strategy.index.into()),
        has_one(manager),
        has_one(usdc_mint) @ VaultError::InvalidUsdcMint,
    )]
    pub strategy: Account<Strategy>,

    pub usdc_mint: Account<Mint>,

    #[account(mut)]
    pub sell_mint: Account<Mint>,
    #[account(mut)]
    pub buy_mint: Account<Mint>,

    #[account(address = AssetConfig::seeds(strategy.address(), sell_config.index))]
    pub sell_config: Account<AssetConfig>,
    #[account(address = AssetConfig::seeds(strategy.address(), buy_config.index))]
    pub buy_config: Account<AssetConfig>,

    /// Pyth feed - validated against the sell asset's registered feed.
    pub sell_price_feed: UncheckedAccount,
    /// Pyth feed - validated against the buy asset's registered feed.
    pub buy_price_feed: UncheckedAccount,

    #[account(mut)]
    pub vault_sell: Account<Token>,
    #[account(mut)]
    pub vault_buy: Account<Token>,
    #[account(mut, address = crate::state::UsdcVaultPda::seeds(strategy.address()))]
    pub vault_usdc: InterfaceAccount<Token>,

    /// Router rate accounts for the sell and buy assets.
    pub sell_rate: UncheckedAccount,
    pub buy_rate: UncheckedAccount,

    #[account(mut)]
    pub router_config: UncheckedAccount,
    #[account(mut)]
    pub router_usdc_treasury: UncheckedAccount,
    pub router_authority: UncheckedAccount,
    pub swap_router_program: UncheckedAccount,

    pub token_program: Program<TokenProgram>,
    pub system_program: Program<SystemProgram>,
}

#[inline(always)]
pub fn handle_rebalance(
    accounts: &mut RebalanceAccountConstraints,
    sell_amount: u64,
    usdc_to_invest: u64,
) -> Result<(), ProgramError> {
    require!(
        *accounts.sell_mint.address() != *accounts.buy_mint.address(),
        VaultError::SameMint
    );

    // Bind each config to its declared mint and vault.
    require_keys_eq!(
        accounts.sell_config.mint,
        *accounts.sell_mint.address(),
        VaultError::AssetNotFound
    );
    require_keys_eq!(
        accounts.sell_config.vault,
        *accounts.vault_sell.address(),
        VaultError::InvalidAssetAccount
    );
    require_keys_eq!(
        accounts.buy_config.mint,
        *accounts.buy_mint.address(),
        VaultError::AssetNotFound
    );
    require_keys_eq!(
        accounts.buy_config.vault,
        *accounts.vault_buy.address(),
        VaultError::InvalidAssetAccount
    );

    let strategy_index = u64::from(accounts.strategy.index);
    let strategy_bump = accounts.strategy.bump;
    let slip = (10_000 - u16::from(accounts.strategy.max_slippage_bps)) as u128;
    let router_program_addr = *accounts.swap_router_program.to_account_view().address();
    require_keys_eq!(
        router_program_addr,
        accounts.strategy.swap_router,
        VaultError::InvalidSwapRouter
    );

    let now = i64::from(Clock::get()?.unix_timestamp);
    let price_sell = load_price(
        accounts.sell_price_feed.to_account_view(),
        &accounts.sell_config.price_feed,
        now,
    )?;
    let price_buy = load_price(
        accounts.buy_price_feed.to_account_view(),
        &accounts.buy_config.price_feed,
        now,
    )?;

    // Sell leg floor: USDC out within slippage of the oracle value of what we sell.
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

    // Buy leg floor: asset out within slippage of the oracle-implied amount.
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
    let bump = [strategy_bump];
    let seeds = [
        Seed::from(STRATEGY_SEED),
        Seed::from(index_bytes.as_ref()),
        Seed::from(bump.as_ref()),
    ];

    // Step 1: sell the basket token for USDC. Router `swap_asset_for_usdc`
    // order: caller, router_config, asset_rate, usdc_mint, asset_mint,
    // caller_asset_account, caller_usdc_account, router_usdc_treasury,
    // router_authority, token_program.
    let mut sell_data = [0u8; SWAP_DATA_LEN];
    sell_data[0] = ROUTER_SWAP_ASSET_FOR_USDC;
    sell_data[1..9].copy_from_slice(&sell_amount.to_le_bytes());
    sell_data[9..17].copy_from_slice(&minimum_usdc_from_sell.to_le_bytes());
    let mut sell_cpi = CpiDynamic::<SWAP_ACCOUNTS, SWAP_DATA_LEN>::new(&router_program_addr);
    sell_cpi.push_account(accounts.strategy.to_account_view(), true, false)?;
    sell_cpi.push_account(accounts.router_config.to_account_view(), false, false)?;
    sell_cpi.push_account(accounts.sell_rate.to_account_view(), false, false)?;
    sell_cpi.push_account(accounts.usdc_mint.to_account_view(), false, false)?;
    sell_cpi.push_account(accounts.sell_mint.to_account_view(), false, true)?;
    sell_cpi.push_account(accounts.vault_sell.to_account_view(), false, true)?;
    sell_cpi.push_account(accounts.vault_usdc.to_account_view(), false, true)?;
    sell_cpi.push_account(accounts.router_usdc_treasury.to_account_view(), false, true)?;
    sell_cpi.push_account(accounts.router_authority.to_account_view(), false, false)?;
    sell_cpi.push_account(accounts.token_program.to_account_view(), false, false)?;
    sell_cpi.set_data(&sell_data)?;
    sell_cpi.invoke_signed(&seeds)?;

    // Step 2: buy the basket token with USDC. Router `swap_usdc_for_asset`
    // order: caller, router_config, asset_rate, usdc_mint, asset_mint,
    // caller_usdc_account, caller_asset_account, router_usdc_treasury,
    // router_authority, token_program.
    let mut buy_data = [0u8; SWAP_DATA_LEN];
    buy_data[0] = ROUTER_SWAP_USDC_FOR_ASSET;
    buy_data[1..9].copy_from_slice(&usdc_to_invest.to_le_bytes());
    buy_data[9..17].copy_from_slice(&minimum_buy_amount.to_le_bytes());
    let mut buy_cpi = CpiDynamic::<SWAP_ACCOUNTS, SWAP_DATA_LEN>::new(&router_program_addr);
    buy_cpi.push_account(accounts.strategy.to_account_view(), true, false)?;
    buy_cpi.push_account(accounts.router_config.to_account_view(), false, false)?;
    buy_cpi.push_account(accounts.buy_rate.to_account_view(), false, false)?;
    buy_cpi.push_account(accounts.usdc_mint.to_account_view(), false, false)?;
    buy_cpi.push_account(accounts.buy_mint.to_account_view(), false, true)?;
    buy_cpi.push_account(accounts.vault_usdc.to_account_view(), false, true)?;
    buy_cpi.push_account(accounts.vault_buy.to_account_view(), false, true)?;
    buy_cpi.push_account(accounts.router_usdc_treasury.to_account_view(), false, true)?;
    buy_cpi.push_account(accounts.router_authority.to_account_view(), false, false)?;
    buy_cpi.push_account(accounts.token_program.to_account_view(), false, false)?;
    buy_cpi.set_data(&buy_data)?;
    buy_cpi.invoke_signed(&seeds)?;

    Ok(())
}
