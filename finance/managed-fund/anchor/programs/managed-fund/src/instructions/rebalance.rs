use anchor_lang::prelude::*;
use anchor_spl::{
    associated_token::AssociatedToken,
    token_interface::{Mint, TokenAccount, TokenInterface},
};
use mock_swap_router::cpi::accounts::{
    SwapAssetForUsdcAccountConstraints as RouterSellAccounts,
    SwapUsdcForAssetAccountConstraints as RouterBuyAccounts,
};

use crate::error::FundError;
use crate::oracle::{
    asset_value_in_usdc, asset_value_share_in_usdc_rounded_up, load_price, read_token_amount,
    usdc_to_asset_amount, OraclePrice,
};
use crate::state::{AssetConfig, Fund, MAX_ASSETS};

#[derive(Accounts)]
pub struct RebalanceAccountConstraints {
    /// Anyone. The program computes the trade from the oracle and the target
    /// weights, so the caller chooses nothing but which pair to restore, and
    /// pays the transaction fee.
    pub caller: Signer,

    #[account(
        mut,
        seeds = [b"fund", fund.index.to_le_bytes()],
        bump = fund.bump,
    )]
    pub fund: Box<BorshAccount<Fund>>,

    #[account(address = fund.usdc_mint @ FundError::InvalidUsdcMint)]
    pub usdc_mint: Box<InterfaceAccount<Mint>>,

    #[account(
        mut,
        associated_token::mint = usdc_mint,
        associated_token::authority = fund,
        associated_token::token_program = token_program
    )]
    pub vault_usdc: Box<InterfaceAccount<TokenAccount>>,

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
pub fn handle_rebalance(
    context: &mut Context<RebalanceAccountConstraints>,
    sell_index: u8,
    buy_index: u8,
) -> Result<()> {
    require!(sell_index != buy_index, FundError::SameMint);

    let fund = &context.accounts.fund;
    let fund_key = *fund.address();
    let fund_index = fund.index;
    let fund_bump = fund.bump;
    let slip = (10_000 - fund.max_slippage_bps) as u128;
    let threshold_bps = fund.rebalance_threshold_bps as u128;
    let usdc_decimals = fund.usdc_decimals;
    let asset_count = fund.asset_count as usize;
    // Recorded holdings, never vault balances: tokens donated into a vault
    // cannot make an asset look over its target and force a trade.
    let mut usdc_holdings = fund.usdc_holdings;
    let mut asset_holdings = fund.asset_holdings;

    let sell = sell_index as usize;
    let buy = buy_index as usize;
    require!(
        sell < asset_count && buy < asset_count,
        FundError::InvalidAssetAccount
    );

    let remaining = context.remaining_accounts()?;
    require!(
        remaining.len() == asset_count * 5,
        FundError::IncompleteAssetAccounts
    );

    let now = Clock::get()?.unix_timestamp;

    // Value every asset, exactly as deposit does.
    let mut nav: u128 = usdc_holdings as u128;
    let mut values = [0u128; MAX_ASSETS as usize];
    let mut sell_asset: Option<(AssetConfig, OraclePrice)> = None;
    let mut buy_asset: Option<(AssetConfig, OraclePrice)> = None;
    for index in 0..asset_count {
        let config = AssetConfig::load_checked(&remaining[index * 5])?;
        require_keys_eq!(config.fund, fund_key, FundError::InvalidAssetAccount);
        require!(
            config.index as usize == index,
            FundError::InvalidAssetAccount
        );
        require_keys_eq!(
            *remaining[index * 5 + 1].address(),
            config.vault,
            FundError::InvalidAssetAccount
        );
        require_keys_eq!(
            *remaining[index * 5 + 2].address(),
            config.mint,
            FundError::InvalidAssetAccount
        );

        let price = load_price(&remaining[index * 5 + 4], &config.price_feed, now)?;
        values[index] = asset_value_in_usdc(
            asset_holdings[index] as u128,
            price,
            config.decimals,
            usdc_decimals,
        )?;
        nav = nav
            .checked_add(values[index])
            .ok_or(FundError::MathOverflow)?;

        if index == sell {
            sell_asset = Some((config, price));
        } else if index == buy {
            buy_asset = Some((config, price));
        }
    }
    let (sell_config, sell_price) = sell_asset.ok_or(FundError::InvalidAssetAccount)?;
    let (buy_config, buy_price) = buy_asset.ok_or(FundError::InvalidAssetAccount)?;

    let target = |weight_bps: u16| -> Result<u128> {
        nav.checked_mul(weight_bps as u128)
            .ok_or(FundError::MathOverflow)?
            .checked_div(10_000)
            .ok_or(FundError::MathOverflow.into())
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

    // Sell leg floor: USDC out within slippage of the oracle value of what is
    // sold, rounded up in the fund's favour so the floor is never looser than
    // the tolerance.
    let minimum_usdc_from_sell: u64 = asset_value_share_in_usdc_rounded_up(
        sell_amount as u128,
        sell_price,
        sell_config.decimals,
        usdc_decimals,
        slip,
    )?
    .try_into()
    .map_err(|_| FundError::MathOverflow)?;

    let index_bytes = fund_index.to_le_bytes();
    let signer_seeds: &[&[&[u8]]] = &[&[b"fund", index_bytes.as_ref(), &[fund_bump]]];

    // `fund` signs the CPIs below. It is a data account holding a live
    // borrow on its buffer, which the runtime would reject when the CPI borrows
    // the same account, so hand the borrow back for the duration.
    context.accounts.fund.release_borrow()?;

    let mut vault_sell = remaining[sell * 5 + 1];
    let mut mint_sell = remaining[sell * 5 + 2];
    let rate_sell = &remaining[sell * 5 + 3];
    let mut vault_buy = remaining[buy * 5 + 1];
    let mut mint_buy = remaining[buy * 5 + 2];
    let rate_buy = &remaining[buy * 5 + 3];

    let sell_before = read_token_amount(&vault_sell)?;
    let usdc_before_sell = context.accounts.vault_usdc.amount();

    // Step 1: sell the over-weight asset for USDC.
    let sell_cpi_accounts = RouterSellAccounts {
        caller: context.accounts.fund.to_cpi_handle(),
        router_config: context.accounts.router_config.cpi_handle(),
        asset_rate: CpiHandle::readonly(rate_sell),
        usdc_mint: context.accounts.usdc_mint.to_cpi_handle(),
        asset_mint: CpiHandleMut::writable(&mut mint_sell),
        caller_asset_account: CpiHandleMut::writable(&mut vault_sell),
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

    // Record what the swap actually moved, measured on the vaults.
    let sold = sell_before
        .checked_sub(read_token_amount(&vault_sell)?)
        .ok_or(FundError::MathOverflow)?;
    let usdc_after_sell = context.accounts.vault_usdc.amount();
    let usdc_received = usdc_after_sell
        .checked_sub(usdc_before_sell)
        .ok_or(FundError::MathOverflow)?;
    asset_holdings[sell] = asset_holdings[sell]
        .checked_sub(sold)
        .ok_or(FundError::InsufficientHoldings)?;
    usdc_holdings = usdc_holdings
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

    let buy_before = read_token_amount(&vault_buy)?;

    let buy_cpi_accounts = RouterBuyAccounts {
        caller: context.accounts.fund.to_cpi_handle(),
        router_config: context.accounts.router_config.cpi_handle(),
        asset_rate: CpiHandle::readonly(rate_buy),
        usdc_mint: context.accounts.usdc_mint.to_cpi_handle(),
        asset_mint: CpiHandleMut::writable(&mut mint_buy),
        caller_usdc_account: context.accounts.vault_usdc.to_cpi_handle_mut(),
        caller_asset_account: CpiHandleMut::writable(&mut vault_buy),
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

    let bought = read_token_amount(&vault_buy)?
        .checked_sub(buy_before)
        .ok_or(FundError::MathOverflow)?;
    let usdc_spent = usdc_after_sell
        .checked_sub(context.accounts.vault_usdc.amount())
        .ok_or(FundError::MathOverflow)?;
    asset_holdings[buy] = asset_holdings[buy]
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
