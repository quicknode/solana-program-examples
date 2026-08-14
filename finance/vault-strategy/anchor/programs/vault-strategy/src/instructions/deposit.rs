use anchor_lang::prelude::*;
use anchor_spl::{
    associated_token::AssociatedToken,
    token_interface::{
        mint_to, transfer_checked, Mint, MintTo, TokenAccount, TokenInterface, TransferChecked,
    },
};
use mock_swap_router::cpi::accounts::SwapUsdcForAssetAccountConstraints as RouterSwapAccounts;

use crate::error::VaultError;
use crate::oracle::{asset_value_in_usdc, load_price, read_token_amount, PYTH_PRICE_PRECISION};
use crate::state::{AssetConfig, Strategy};

#[derive(Accounts)]
pub struct DepositAccountConstraints {
    #[account(mut)]
    pub depositor: Signer,

    #[account(
        mut,
        has_one = usdc_mint @ VaultError::InvalidUsdcMint,
        seeds = [b"strategy", strategy.index.to_le_bytes()],
        bump = strategy.bump
    )]
    pub strategy: Box<BorshAccount<Strategy>>,

    #[account(
        mut,
        seeds = [b"share_mint", strategy.address().as_ref()],
        bump
    )]
    pub share_mint: Box<InterfaceAccount<Mint>>,

    pub usdc_mint: Box<InterfaceAccount<Mint>>,

    #[account(
        mut,
        associated_token::mint = usdc_mint,
        associated_token::authority = depositor,
        associated_token::token_program = token_program
    )]
    pub depositor_usdc_account: Box<InterfaceAccount<TokenAccount>>,

    #[account(
        init_if_needed,
        payer = depositor,
        associated_token::mint = share_mint,
        associated_token::authority = depositor,
        associated_token::token_program = token_program
    )]
    pub depositor_share_account: Box<InterfaceAccount<TokenAccount>>,

    #[account(
        mut,
        associated_token::mint = usdc_mint,
        associated_token::authority = strategy,
        associated_token::token_program = token_program
    )]
    pub vault_usdc: Box<InterfaceAccount<TokenAccount>>,

    /// CHECK: Router config PDA from the mock-swap-router program
    #[account(mut)]
    pub router_config: UncheckedAccount,

    /// CHECK: Router USDC treasury ATA
    #[account(mut)]
    pub router_usdc_treasury: UncheckedAccount,

    /// CHECK: Router authority PDA from the mock-swap-router program
    #[account(mut)]
    pub router_authority: UncheckedAccount,

    #[account(
        constraint = swap_router_program.address() == strategy.swap_router @ VaultError::InvalidSwapRouter
    )]
    pub swap_router_program: Program<mock_swap_router::program::MockSwapRouter>,

    pub associated_token_program: Program<AssociatedToken>,
    pub token_program: Interface<'static, TokenInterface>,
    pub system_program: Program<System>,
    // remaining_accounts: for each asset index 0..asset_count, in order:
    //   [asset_config, vault, asset_mint, asset_rate, price_feed]
}

/// Deposit USDC, receive shares priced at net asset value, and immediately deploy
/// the deposit into the basket at its target weights. The strategy must be fully
/// allocated first: the weights sum to exactly 10000, so every deposit is fully
/// invested. For each asset the handler swaps `usdc_amount * weight_bps / 10000`
/// through the registered router, so a depositor's money is invested in the same
/// transaction they put it in (only sub-cent rounding dust can remain as USDC).
pub fn handle_deposit<'info>(
    context: &mut Context<'info, DepositAccountConstraints<'info>>,
    usdc_amount: u64,
    minimum_shares: u64,
) -> Result<()> {
    require!(usdc_amount > 0, VaultError::ZeroDeposit);
    // A strategy accepts deposits only once its weights sum to 100%, so a deposit is
    // always fully invested. A half-configured or under-allocated basket is closed.
    require!(
        context.accounts.strategy.total_weight_bps == 10_000,
        VaultError::StrategyNotFullyAllocated
    );

    let vault_usdc_amount = context.accounts.vault_usdc.amount();
    let total_shares = context.accounts.strategy.total_shares;
    let usdc_decimals = context.accounts.usdc_mint.decimals();
    let strategy_index = context.accounts.strategy.index;
    let strategy_bump = context.accounts.strategy.bump;
    let strategy_key = context.accounts.strategy.address();
    let max_slippage_bps = context.accounts.strategy.max_slippage_bps;
    let asset_count = context.accounts.strategy.asset_count as usize;

    let now = Clock::get()?.unix_timestamp;

    // Net asset value over the complete asset set. The assets are exactly indices
    // 0..asset_count, so requiring five accounts per index, in order, each with a
    // matching index, makes it impossible to omit an asset and understate NAV.
    let remaining = context.remaining_accounts();
    require!(
        remaining.len() == asset_count * 5,
        VaultError::IncompleteAssetAccounts
    );

    let mut nav: u128 = vault_usdc_amount as u128;

    for index in 0..asset_count {
        let config_account = &remaining[index * 5];
        let vault_account = &remaining[index * 5 + 1];
        let feed_account = &remaining[index * 5 + 4];

        let config = AssetConfig::load_checked(config_account)?;
        require_keys_eq!(
            config.strategy,
            strategy_key,
            VaultError::InvalidAssetAccount
        );
        require!(
            config.index as usize == index,
            VaultError::InvalidAssetAccount
        );
        require_keys_eq!(
            vault_account.address(),
            config.vault,
            VaultError::InvalidAssetAccount
        );

        let price = load_price(feed_account, &config.price_feed, now)?;
        let amount = read_token_amount(vault_account)?;
        nav = nav
            .checked_add(asset_value_in_usdc(amount, price)?)
            .ok_or(VaultError::MathOverflow)?;
    }

    // shares = usdc_amount * total_shares / nav (floor); first deposit is 1:1.
    let shares_to_mint: u64 = if total_shares == 0 {
        usdc_amount
    } else {
        (usdc_amount as u128)
            .checked_mul(total_shares as u128)
            .ok_or(VaultError::MathOverflow)?
            .checked_div(nav)
            .ok_or(VaultError::MathOverflow)? as u64
    };

    require!(
        shares_to_mint >= minimum_shares,
        VaultError::SlippageTooHigh
    );

    context.accounts.strategy.total_shares = total_shares
        .checked_add(shares_to_mint)
        .ok_or(VaultError::MathOverflow)?;

    // Pull the depositor's USDC into the strategy's USDC vault.
    let transfer_accounts = TransferChecked {
        from: context.accounts.depositor_usdc_account.cpi_handle_mut(),
        mint: context.accounts.usdc_mint.cpi_handle_mut(),
        to: context.accounts.vault_usdc.cpi_handle_mut(),
        authority: context.accounts.depositor.cpi_handle_mut(),
    };
    let cpi_ctx = CpiContext::new(context.accounts.token_program.address(), transfer_accounts);
    transfer_checked(cpi_ctx, usdc_amount, usdc_decimals)?;

    let index_bytes = strategy_index.to_le_bytes();
    let signer_seeds: &[&[&[u8]]] = &[&[b"strategy", index_bytes.as_ref(), &[strategy_bump]]];

    // Deploy the deposit across the basket at its target weights. Each leg swaps a
    // weight-sized slice of the deposit through the router, under an oracle-computed
    // slippage floor. The strategy PDA signs, since the USDC leaves a vault only it
    // controls.
    for index in 0..asset_count {
        let config_account = &remaining[index * 5];
        let vault_account = &remaining[index * 5 + 1];
        let mint_account = &remaining[index * 5 + 2];
        let rate_account = &remaining[index * 5 + 3];
        let feed_account = &remaining[index * 5 + 4];

        let config = AssetConfig::load_checked(config_account)?;
        require_keys_eq!(
            mint_account.address(),
            config.mint,
            VaultError::InvalidAssetAccount
        );

        if config.weight_bps == 0 {
            continue;
        }

        let deploy_usdc: u64 = (usdc_amount as u128)
            .checked_mul(config.weight_bps as u128)
            .ok_or(VaultError::MathOverflow)?
            .checked_div(10_000)
            .ok_or(VaultError::MathOverflow)? as u64;

        if deploy_usdc == 0 {
            continue;
        }

        // Slippage floor anchored to the oracle: expected_out = deploy_usdc * 10^8 /
        // price, allowed to fall short by at most max_slippage_bps.
        let price = load_price(feed_account, &config.price_feed, now)?;
        let expected_out = (deploy_usdc as u128)
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

        let cpi_accounts = RouterSwapAccounts {
            caller: context.accounts.strategy.cpi_handle_mut(),
            router_config: context.accounts.router_config.cpi_handle_mut(),
            asset_rate: rate_account.clone(),
            usdc_mint: context.accounts.usdc_mint.cpi_handle_mut(),
            asset_mint: mint_account.clone(),
            caller_usdc_account: context.accounts.vault_usdc.cpi_handle_mut(),
            caller_asset_account: vault_account.clone(),
            router_usdc_treasury: context.accounts.router_usdc_treasury.cpi_handle_mut(),
            router_authority: context.accounts.router_authority.cpi_handle_mut(),
            associated_token_program: context.accounts.associated_token_program.cpi_handle_mut(),
            token_program: context.accounts.token_program.cpi_handle_mut(),
            system_program: context.accounts.system_program.cpi_handle_mut(),
        };
        let cpi_ctx = CpiContext::new_with_signer(
            context.accounts.swap_router_program.address(),
            cpi_accounts,
            signer_seeds,
        );
        mock_swap_router::cpi::swap_usdc_for_asset(cpi_ctx, deploy_usdc, minimum_asset_out)?;
    }

    // Mint the shares last, with the strategy PDA signing as the share mint authority.
    let mint_accounts = MintTo {
        mint: context.accounts.share_mint.cpi_handle_mut(),
        to: context.accounts.depositor_share_account.cpi_handle_mut(),
        authority: context.accounts.strategy.cpi_handle_mut(),
    };
    let cpi_ctx = CpiContext::new_with_signer(
        context.accounts.token_program.address(),
        mint_accounts,
        signer_seeds,
    );
    mint_to(cpi_ctx, shares_to_mint)?;

    Ok(())
}
