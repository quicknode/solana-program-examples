use quasar_lang::cpi::Seed;
use quasar_lang::prelude::*;
use quasar_lang::remaining::RemainingAccounts;
use quasar_lang::sysvars::Sysvar as _;
use quasar_spl::prelude::*;

use crate::errors::FundError;
use crate::instructions::deposit::get_view;
use crate::oracle::{
    asset_value_in_usdc, load_price, read_token_amount, usdc_to_asset_amount, OraclePrice,
};
use crate::state::{
    load_asset_config, read_asset_holdings, snapshot_fund, write_asset_holdings, AssetConfigView,
    Fund, UsdcVaultPda, FUND_SEED, MAX_ASSETS,
};

const ROUTER_SWAP_USDC_FOR_ASSET: u8 = 2;
const ROUTER_SWAP_ASSET_FOR_USDC: u8 = 3;
const SWAP_ACCOUNTS: usize = 9;
const SWAP_DATA_LEN: usize = 17;
/// remaining_accounts arrive as, per asset index 0..asset_count:
///   [asset_config, vault, asset_mint, asset_rate, price_feed]
const ACCOUNTS_PER_ASSET: usize = 5;

#[derive(Accounts)]
pub struct RebalanceAccountConstraints {
    /// Anyone. The program computes the trade from the oracle and the target
    /// weights, so the caller chooses nothing but which pair to restore, and
    /// pays the transaction fee.
    pub caller: Signer,

    #[account(
        mut,
        address = Fund::seeds(fund.index.into()),
        has_one(usdc_mint) @ FundError::InvalidUsdcMint,
    )]
    pub fund: Account<Fund>,

    pub usdc_mint: Account<Mint>,

    #[account(mut, address = UsdcVaultPda::seeds(fund.address()))]
    pub vault_usdc: InterfaceAccount<Token>,

    /// Router config PDA; it owns the treasury and signs the router's token CPIs.
    #[account(mut)]
    pub router_config: UncheckedAccount,
    #[account(mut)]
    pub router_usdc_treasury: UncheckedAccount,
    pub swap_router_program: UncheckedAccount,

    pub token_program: Program<TokenProgram>,
    pub system_program: Program<SystemProgram>,
    // remaining_accounts: for each asset index 0..asset_count, in order:
    //   [asset_config, vault, asset_mint, asset_rate, price_feed]
    // the same layout as deposit. Every asset is valued, so the targets are
    // shares of the whole fund.
}

/// Restore one pair of assets toward their target weights. The asset at
/// `sell_index` must sit above its target by at least the fund's
/// `rebalance_threshold_bps` of the fund's value (or be retired, at weight zero,
/// and still held); the asset at `buy_index` must sit below its target. The
/// trade is the smaller of the two gaps, so it never pushes either asset past
/// its target, and a second call straight after finds nothing to do. That is
/// what lets anyone call it: no caller can choose a trade size, and none can
/// trade a fund that has not drifted.
#[inline(always)]
pub fn handle_rebalance(
    accounts: &mut RebalanceAccountConstraints,
    remaining: RemainingAccounts<'_>,
    sell_index: u8,
    buy_index: u8,
) -> Result<(), ProgramError> {
    require!(sell_index != buy_index, FundError::SameMint);

    let fund_key = *accounts.fund.address();
    let fund_index = u64::from(accounts.fund.index);
    let fund_bump = accounts.fund.bump;
    let slip = (10_000 - u16::from(accounts.fund.max_slippage_bps)) as u128;
    let threshold_bps = u16::from(accounts.fund.rebalance_threshold_bps) as u128;
    let usdc_decimals = accounts.fund.usdc_decimals;
    let asset_count = accounts.fund.asset_count as usize;
    let router_program_addr = *accounts.swap_router_program.to_account_view().address();
    require_keys_eq!(
        router_program_addr,
        accounts.fund.swap_router,
        FundError::InvalidSwapRouter
    );
    // Recorded holdings, never vault balances: tokens donated into a vault
    // cannot make an asset look over its target and force a trade.
    let mut fund = snapshot_fund(&accounts.fund);
    let mut asset_holdings = read_asset_holdings(&fund.asset_holdings);

    let sell = sell_index as usize;
    let buy = buy_index as usize;
    require!(
        sell < asset_count && buy < asset_count,
        FundError::InvalidAssetAccount
    );

    // Exactly five accounts per asset - no more, no less - so no asset can be
    // omitted from the valuation.
    require!(
        remaining
            .get(asset_count * ACCOUNTS_PER_ASSET - 1)?
            .is_some()
            && remaining.get(asset_count * ACCOUNTS_PER_ASSET)?.is_none(),
        FundError::IncompleteAssetAccounts
    );

    let now = i64::from(Clock::get()?.unix_timestamp);

    // Value every asset, exactly as deposit does.
    let mut nav: u128 = fund.usdc_holdings as u128;
    let mut values = [0u128; MAX_ASSETS as usize];
    let mut sell_asset: Option<(AssetConfigView, OraclePrice)> = None;
    let mut buy_asset: Option<(AssetConfigView, OraclePrice)> = None;
    for (index, value) in values.iter_mut().enumerate().take(asset_count) {
        let config_view = get_view(&remaining, index * ACCOUNTS_PER_ASSET)?;
        let vault_view = get_view(&remaining, index * ACCOUNTS_PER_ASSET + 1)?;
        let mint_view = get_view(&remaining, index * ACCOUNTS_PER_ASSET + 2)?;
        let feed_view = get_view(&remaining, index * ACCOUNTS_PER_ASSET + 4)?;

        let config = load_asset_config(&config_view)?;
        require_keys_eq!(config.fund, fund_key, FundError::InvalidAssetAccount);
        require!(
            config.index as usize == index,
            FundError::InvalidAssetAccount
        );
        require_keys_eq!(
            *vault_view.address(),
            config.vault,
            FundError::InvalidAssetAccount
        );
        require_keys_eq!(
            *mint_view.address(),
            config.mint,
            FundError::InvalidAssetAccount
        );

        let price = load_price(&feed_view, &config.price_feed, now)?;
        *value = asset_value_in_usdc(
            asset_holdings[index] as u128,
            price,
            config.decimals,
            usdc_decimals,
        )?;
        nav = nav.checked_add(*value).ok_or(FundError::MathOverflow)?;

        if index == sell {
            sell_asset = Some((config, price));
        } else if index == buy {
            buy_asset = Some((config, price));
        }
    }
    let (sell_config, sell_price) = sell_asset.ok_or(FundError::InvalidAssetAccount)?;
    let (buy_config, buy_price) = buy_asset.ok_or(FundError::InvalidAssetAccount)?;

    let target = |weight_bps: u16| -> Result<u128, ProgramError> {
        nav.checked_mul(weight_bps as u128)
            .ok_or(FundError::MathOverflow)?
            .checked_div(10_000)
            .ok_or_else(|| FundError::MathOverflow.into())
    };

    // The sell side: how far above its target the asset sits. A retired asset
    // may always be sold down; any other only once the gap reaches the
    // threshold, so the fund does not trade on every small price move.
    let excess = values[sell].saturating_sub(target(sell_config.weight_bps)?);
    let threshold_value = nav
        .checked_mul(threshold_bps)
        .ok_or(FundError::MathOverflow)?
        .checked_div(10_000)
        .ok_or(FundError::MathOverflow)?;
    require!(
        excess > 0 && (sell_config.weight_bps == 0 || excess >= threshold_value),
        FundError::DriftBelowThreshold
    );

    // The buy side: how far below its target the asset sits.
    let shortfall = target(buy_config.weight_bps)?.saturating_sub(values[buy]);
    require!(shortfall > 0, FundError::NotUnderweight);

    // Trade the smaller gap, so neither asset ends up past its target.
    let trade_value = excess.min(shortfall);
    let sell_amount: u64 =
        usdc_to_asset_amount(trade_value, sell_price, sell_config.decimals, usdc_decimals)?
            .try_into()
            .map_err(|_| FundError::MathOverflow)?;
    require!(sell_amount > 0, FundError::DriftBelowThreshold);
    require!(
        sell_amount <= asset_holdings[sell],
        FundError::InsufficientHoldings
    );

    // Sell leg floor: USDC out within slippage of the oracle value of what is sold.
    let minimum_usdc_from_sell: u64 = asset_value_in_usdc(
        sell_amount as u128,
        sell_price,
        sell_config.decimals,
        usdc_decimals,
    )?
    .checked_mul(slip)
    .ok_or(FundError::MathOverflow)?
    .checked_div(10_000)
    .ok_or(FundError::MathOverflow)?
    .try_into()
    .map_err(|_| FundError::MathOverflow)?;

    let index_bytes = fund_index.to_le_bytes();
    let bump = [fund_bump];
    let seeds = [
        Seed::from(FUND_SEED),
        Seed::from(index_bytes.as_ref()),
        Seed::from(bump.as_ref()),
    ];

    let vault_sell = get_view(&remaining, sell * ACCOUNTS_PER_ASSET + 1)?;
    let mint_sell = get_view(&remaining, sell * ACCOUNTS_PER_ASSET + 2)?;
    let rate_sell = get_view(&remaining, sell * ACCOUNTS_PER_ASSET + 3)?;
    let vault_buy = get_view(&remaining, buy * ACCOUNTS_PER_ASSET + 1)?;
    let mint_buy = get_view(&remaining, buy * ACCOUNTS_PER_ASSET + 2)?;
    let rate_buy = get_view(&remaining, buy * ACCOUNTS_PER_ASSET + 3)?;

    // Step 1: sell the over-weight asset for USDC. Router `swap_asset_for_usdc`
    // order: caller, router_config, asset_rate, usdc_mint, asset_mint,
    // caller_asset_account, caller_usdc_account, router_usdc_treasury,
    // token_program.
    let mut sell_data = [0u8; SWAP_DATA_LEN];
    sell_data[0] = ROUTER_SWAP_ASSET_FOR_USDC;
    sell_data[1..9].copy_from_slice(&sell_amount.to_le_bytes());
    sell_data[9..17].copy_from_slice(&minimum_usdc_from_sell.to_le_bytes());
    let mut sell_cpi = CpiDynamic::<SWAP_ACCOUNTS, SWAP_DATA_LEN>::new(&router_program_addr);
    sell_cpi.push_account(accounts.fund.to_account_view(), true, false)?;
    sell_cpi.push_account(accounts.router_config.to_account_view(), false, false)?;
    sell_cpi.push_account(&rate_sell, false, false)?;
    sell_cpi.push_account(accounts.usdc_mint.to_account_view(), false, false)?;
    sell_cpi.push_account(&mint_sell, false, true)?;
    sell_cpi.push_account(&vault_sell, false, true)?;
    sell_cpi.push_account(accounts.vault_usdc.to_account_view(), false, true)?;
    sell_cpi.push_account(accounts.router_usdc_treasury.to_account_view(), false, true)?;
    sell_cpi.push_account(accounts.token_program.to_account_view(), false, false)?;
    sell_cpi.set_data(&sell_data)?;
    let sell_before = read_token_amount(&vault_sell)?;
    let usdc_before_sell = read_token_amount(accounts.vault_usdc.to_account_view())?;
    sell_cpi.invoke_signed(&seeds)?;

    // Record what the swap actually moved, measured on the vaults.
    let sold = sell_before
        .checked_sub(read_token_amount(&vault_sell)?)
        .ok_or(FundError::MathOverflow)?;
    let usdc_after_sell = read_token_amount(accounts.vault_usdc.to_account_view())?;
    let usdc_received = usdc_after_sell
        .checked_sub(usdc_before_sell)
        .ok_or(FundError::MathOverflow)?;
    asset_holdings[sell] = asset_holdings[sell]
        .checked_sub(sold)
        .ok_or(FundError::InsufficientHoldings)?;
    fund.usdc_holdings = fund
        .usdc_holdings
        .checked_add(usdc_received)
        .ok_or(FundError::MathOverflow)?;

    // Step 2: spend exactly what the sale brought in on the under-weight asset.
    // Donated USDC in the vault is never spent, because only the sale's proceeds
    // are invested.
    let usdc_to_invest = usdc_received;
    let minimum_buy_amount: u64 = usdc_to_asset_amount(
        usdc_to_invest as u128,
        buy_price,
        buy_config.decimals,
        usdc_decimals,
    )?
    .checked_mul(slip)
    .ok_or(FundError::MathOverflow)?
    .checked_div(10_000)
    .ok_or(FundError::MathOverflow)?
    .try_into()
    .map_err(|_| FundError::MathOverflow)?;

    // Router `swap_usdc_for_asset` order: caller, router_config, asset_rate,
    // usdc_mint, asset_mint, caller_usdc_account, caller_asset_account,
    // router_usdc_treasury, token_program.
    let mut buy_data = [0u8; SWAP_DATA_LEN];
    buy_data[0] = ROUTER_SWAP_USDC_FOR_ASSET;
    buy_data[1..9].copy_from_slice(&usdc_to_invest.to_le_bytes());
    buy_data[9..17].copy_from_slice(&minimum_buy_amount.to_le_bytes());
    let mut buy_cpi = CpiDynamic::<SWAP_ACCOUNTS, SWAP_DATA_LEN>::new(&router_program_addr);
    buy_cpi.push_account(accounts.fund.to_account_view(), true, false)?;
    buy_cpi.push_account(accounts.router_config.to_account_view(), false, false)?;
    buy_cpi.push_account(&rate_buy, false, false)?;
    buy_cpi.push_account(accounts.usdc_mint.to_account_view(), false, false)?;
    buy_cpi.push_account(&mint_buy, false, true)?;
    buy_cpi.push_account(accounts.vault_usdc.to_account_view(), false, true)?;
    buy_cpi.push_account(&vault_buy, false, true)?;
    buy_cpi.push_account(accounts.router_usdc_treasury.to_account_view(), false, true)?;
    buy_cpi.push_account(accounts.token_program.to_account_view(), false, false)?;
    buy_cpi.set_data(&buy_data)?;
    let buy_before = read_token_amount(&vault_buy)?;
    buy_cpi.invoke_signed(&seeds)?;

    let bought = read_token_amount(&vault_buy)?
        .checked_sub(buy_before)
        .ok_or(FundError::MathOverflow)?;
    let usdc_spent = usdc_after_sell
        .checked_sub(read_token_amount(accounts.vault_usdc.to_account_view())?)
        .ok_or(FundError::MathOverflow)?;
    asset_holdings[buy] = asset_holdings[buy]
        .checked_add(bought)
        .ok_or(FundError::MathOverflow)?;
    fund.usdc_holdings = fund
        .usdc_holdings
        .checked_sub(usdc_spent)
        .ok_or(FundError::InsufficientHoldings)?;
    fund.asset_holdings = write_asset_holdings(&asset_holdings);
    accounts.fund.set_inner(fund);

    Ok(())
}
