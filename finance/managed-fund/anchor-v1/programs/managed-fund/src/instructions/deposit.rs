use anchor_lang::prelude::*;
use anchor_spl::{
    associated_token::AssociatedToken,
    token_interface::{
        mint_to, transfer_checked, Mint, MintTo, TokenAccount, TokenInterface, TransferChecked,
    },
};
use mock_swap_router::cpi::accounts::SwapUsdcForAssetAccountConstraints as RouterSwapAccounts;

use crate::error::FundError;
use crate::oracle::{asset_value_in_usdc, load_price, read_token_amount, PYTH_PRICE_PRECISION};
use crate::state::{AssetConfig, Fund};

#[derive(Accounts)]
pub struct DepositAccountConstraints<'info> {
    #[account(mut)]
    pub depositor: Signer<'info>,

    #[account(
        mut,
        has_one = usdc_mint @ FundError::InvalidUsdcMint,
        seeds = [b"fund", fund.index.to_le_bytes().as_ref()],
        bump = fund.bump
    )]
    pub fund: Box<Account<'info, Fund>>,

    #[account(
        mut,
        seeds = [b"share_mint", fund.key().as_ref()],
        bump
    )]
    pub share_mint: Box<InterfaceAccount<'info, Mint>>,

    pub usdc_mint: Box<InterfaceAccount<'info, Mint>>,

    #[account(
        mut,
        associated_token::mint = usdc_mint,
        associated_token::authority = depositor,
        associated_token::token_program = token_program
    )]
    pub depositor_usdc_account: Box<InterfaceAccount<'info, TokenAccount>>,

    #[account(
        init_if_needed,
        payer = depositor,
        associated_token::mint = share_mint,
        associated_token::authority = depositor,
        associated_token::token_program = token_program
    )]
    pub depositor_share_account: Box<InterfaceAccount<'info, TokenAccount>>,

    #[account(
        mut,
        associated_token::mint = usdc_mint,
        associated_token::authority = fund,
        associated_token::token_program = token_program
    )]
    pub vault_usdc: Box<InterfaceAccount<'info, TokenAccount>>,

    /// CHECK: Router config PDA from the mock-swap-router program; it owns the
    /// treasury and signs the router's token CPIs
    #[account(mut)]
    pub router_config: UncheckedAccount<'info>,

    /// CHECK: Router USDC treasury ATA, owned by the router config account
    #[account(mut)]
    pub router_usdc_treasury: UncheckedAccount<'info>,

    #[account(
        constraint = swap_router_program.key() == fund.swap_router @ FundError::InvalidSwapRouter
    )]
    pub swap_router_program: Program<'info, mock_swap_router::program::MockSwapRouter>,

    pub associated_token_program: Program<'info, AssociatedToken>,
    pub token_program: Interface<'info, TokenInterface>,
    pub system_program: Program<'info, System>,
    // remaining_accounts: for each asset index 0..asset_count, in order:
    //   [asset_config, vault, asset_mint, asset_rate, price_feed]
}

/// Deposit USDC, receive shares priced at net asset value, and immediately deploy
/// the deposit into the basket at its target weights. The fund must be fully
/// allocated first: the weights sum to exactly 10000, so every deposit is fully
/// invested. For each asset the handler swaps `usdc_amount * weight_bps / 10000`
/// through the registered router, so a depositor's money is invested in the same
/// transaction they put it in (only sub-cent rounding dust can remain as USDC).
pub fn handle_deposit<'info>(
    context: Context<'info, DepositAccountConstraints<'info>>,
    usdc_amount: u64,
    minimum_shares: u64,
) -> Result<()> {
    require!(usdc_amount > 0, FundError::ZeroDeposit);
    // A fund accepts deposits only once its weights sum to 100%, so a deposit is
    // always fully invested. A half-configured or under-allocated basket is closed.
    require!(
        context.accounts.fund.total_weight_bps == 10_000,
        FundError::FundNotFullyAllocated
    );

    let total_shares = context.accounts.fund.total_shares;
    let usdc_decimals = context.accounts.usdc_mint.decimals;
    let fund_index = context.accounts.fund.index;
    let fund_bump = context.accounts.fund.bump;
    let fund_key = context.accounts.fund.key();
    let max_slippage_bps = context.accounts.fund.max_slippage_bps;
    let asset_count = context.accounts.fund.asset_count as usize;
    // The holdings the program has accounted for, not the vaults' token
    // balances: a donation straight into a vault changes a balance and none of
    // these, so it cannot move the share price.
    let mut usdc_holdings = context.accounts.fund.usdc_holdings;
    let mut asset_holdings = context.accounts.fund.asset_holdings;

    let now = Clock::get()?.unix_timestamp;

    // Net asset value over the complete asset set. The assets are exactly indices
    // 0..asset_count, so requiring five accounts per index, in order, each with a
    // matching index, makes it impossible to omit an asset and understate NAV.
    let remaining = context.remaining_accounts;
    require!(
        remaining.len() == asset_count * 5,
        FundError::IncompleteAssetAccounts
    );

    let mut nav: u128 = usdc_holdings as u128;

    for index in 0..asset_count {
        let config_account = &remaining[index * 5];
        let vault_account = &remaining[index * 5 + 1];
        let feed_account = &remaining[index * 5 + 4];

        let config = AssetConfig::load_checked(config_account)?;
        require_keys_eq!(config.fund, fund_key, FundError::InvalidAssetAccount);
        require!(
            config.index as usize == index,
            FundError::InvalidAssetAccount
        );
        require_keys_eq!(
            vault_account.key(),
            config.vault,
            FundError::InvalidAssetAccount
        );

        let price = load_price(feed_account, &config.price_feed, now)?;
        let amount = asset_holdings[index];
        nav = nav
            .checked_add(asset_value_in_usdc(amount, price)?)
            .ok_or(FundError::MathOverflow)?;
    }

    // shares = usdc_amount * total_shares / nav (floor); first deposit is 1:1.
    let shares_to_mint: u64 = if total_shares == 0 {
        usdc_amount
    } else {
        (usdc_amount as u128)
            .checked_mul(total_shares as u128)
            .ok_or(FundError::MathOverflow)?
            .checked_div(nav)
            .ok_or(FundError::MathOverflow)? as u64
    };

    require!(shares_to_mint >= minimum_shares, FundError::SlippageTooHigh);

    context.accounts.fund.total_shares = total_shares
        .checked_add(shares_to_mint)
        .ok_or(FundError::MathOverflow)?;
    usdc_holdings = usdc_holdings
        .checked_add(usdc_amount)
        .ok_or(FundError::MathOverflow)?;
    let vault_usdc_info = context.accounts.vault_usdc.to_account_info();

    // Pull the depositor's USDC into the fund's USDC vault.
    let transfer_accounts = TransferChecked {
        from: context.accounts.depositor_usdc_account.to_account_info(),
        mint: context.accounts.usdc_mint.to_account_info(),
        to: context.accounts.vault_usdc.to_account_info(),
        authority: context.accounts.depositor.to_account_info(),
    };
    let cpi_ctx = CpiContext::new(context.accounts.token_program.key(), transfer_accounts);
    transfer_checked(cpi_ctx, usdc_amount, usdc_decimals)?;

    let index_bytes = fund_index.to_le_bytes();
    let signer_seeds: &[&[&[u8]]] = &[&[b"fund", index_bytes.as_ref(), &[fund_bump]]];

    // Deploy the deposit across the basket at its target weights. Each leg swaps a
    // weight-sized slice of the deposit through the router, under an oracle-computed
    // slippage floor. The fund PDA signs, since the USDC leaves a vault only it
    // controls.
    for index in 0..asset_count {
        let config_account = &remaining[index * 5];
        let vault_account = &remaining[index * 5 + 1];
        let mint_account = &remaining[index * 5 + 2];
        let rate_account = &remaining[index * 5 + 3];
        let feed_account = &remaining[index * 5 + 4];

        let config = AssetConfig::load_checked(config_account)?;
        require_keys_eq!(
            mint_account.key(),
            config.mint,
            FundError::InvalidAssetAccount
        );

        if config.weight_bps == 0 {
            continue;
        }

        let deploy_usdc: u64 = (usdc_amount as u128)
            .checked_mul(config.weight_bps as u128)
            .ok_or(FundError::MathOverflow)?
            .checked_div(10_000)
            .ok_or(FundError::MathOverflow)? as u64;

        if deploy_usdc == 0 {
            continue;
        }

        // Slippage floor anchored to the oracle: expected_out = deploy_usdc * 10^8 /
        // price, allowed to fall short by at most max_slippage_bps.
        let price = load_price(feed_account, &config.price_feed, now)?;
        let expected_out = (deploy_usdc as u128)
            .checked_mul(PYTH_PRICE_PRECISION)
            .ok_or(FundError::MathOverflow)?
            .checked_div(price)
            .ok_or(FundError::MathOverflow)?;
        let minimum_asset_out: u64 = expected_out
            .checked_mul((10_000 - max_slippage_bps) as u128)
            .ok_or(FundError::MathOverflow)?
            .checked_div(10_000)
            .ok_or(FundError::MathOverflow)?
            .try_into()
            .map_err(|_| FundError::MathOverflow)?;

        // Record what the swap actually moves, measured on the vaults, rather
        // than what was asked for.
        let asset_before = read_token_amount(vault_account)?;
        let usdc_before = read_token_amount(&vault_usdc_info)?;

        let cpi_accounts = RouterSwapAccounts {
            caller: context.accounts.fund.to_account_info(),
            router_config: context.accounts.router_config.to_account_info(),
            asset_rate: rate_account.clone(),
            usdc_mint: context.accounts.usdc_mint.to_account_info(),
            asset_mint: mint_account.clone(),
            caller_usdc_account: context.accounts.vault_usdc.to_account_info(),
            caller_asset_account: vault_account.clone(),
            router_usdc_treasury: context.accounts.router_usdc_treasury.to_account_info(),
            associated_token_program: context.accounts.associated_token_program.to_account_info(),
            token_program: context.accounts.token_program.to_account_info(),
            system_program: context.accounts.system_program.to_account_info(),
        };
        let cpi_ctx = CpiContext::new_with_signer(
            context.accounts.swap_router_program.key(),
            cpi_accounts,
            signer_seeds,
        );
        mock_swap_router::cpi::swap_usdc_for_asset(cpi_ctx, deploy_usdc, minimum_asset_out)?;

        let asset_received = read_token_amount(vault_account)?
            .checked_sub(asset_before)
            .ok_or(FundError::MathOverflow)?;
        let usdc_spent = usdc_before
            .checked_sub(read_token_amount(&vault_usdc_info)?)
            .ok_or(FundError::MathOverflow)?;
        // A leg that spends USDC and buys nothing would leave shares minted
        // against no recorded value, and every later deposit would divide by a
        // zero NAV. Refuse it: the deposit is too small for this basket.
        require!(asset_received > 0, FundError::DepositTooSmall);
        asset_holdings[index] = asset_holdings[index]
            .checked_add(asset_received)
            .ok_or(FundError::MathOverflow)?;
        usdc_holdings = usdc_holdings
            .checked_sub(usdc_spent)
            .ok_or(FundError::MathOverflow)?;
    }
    context.accounts.fund.usdc_holdings = usdc_holdings;
    context.accounts.fund.asset_holdings = asset_holdings;

    // Mint the shares last, with the fund PDA signing as the share mint authority.
    let mint_accounts = MintTo {
        mint: context.accounts.share_mint.to_account_info(),
        to: context.accounts.depositor_share_account.to_account_info(),
        authority: context.accounts.fund.to_account_info(),
    };
    let cpi_ctx = CpiContext::new_with_signer(
        context.accounts.token_program.key(),
        mint_accounts,
        signer_seeds,
    );
    mint_to(cpi_ctx, shares_to_mint)?;

    Ok(())
}
