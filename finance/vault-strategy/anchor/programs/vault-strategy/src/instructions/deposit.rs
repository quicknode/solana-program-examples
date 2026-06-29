use anchor_lang::prelude::*;
use anchor_spl::{
    associated_token::AssociatedToken,
    token_interface::{
        mint_to, transfer_checked, Mint, MintTo, TokenAccount, TokenInterface, TransferChecked,
    },
};

use crate::error::VaultError;
use crate::oracle::{asset_value_in_usdc, load_price, read_token_amount};
use crate::state::{AssetConfig, Strategy};

#[derive(Accounts)]
pub struct DepositAccountConstraints<'info> {
    #[account(mut)]
    pub depositor: Signer<'info>,

    #[account(
        mut,
        has_one = usdc_mint @ VaultError::InvalidUsdcMint,
        seeds = [b"strategy", strategy.manager.as_ref()],
        bump = strategy.bump
    )]
    pub strategy: Box<Account<'info, Strategy>>,

    #[account(
        mut,
        seeds = [b"share_mint", strategy.key().as_ref()],
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
        associated_token::authority = strategy,
        associated_token::token_program = token_program
    )]
    pub vault_usdc: Box<InterfaceAccount<'info, TokenAccount>>,

    pub associated_token_program: Program<'info, AssociatedToken>,
    pub token_program: Interface<'info, TokenInterface>,
    pub system_program: Program<'info, System>,
    // remaining_accounts: for each asset index 0..asset_count, in order:
    //   [asset_config, vault, price_feed]
}

pub fn handle_deposit<'info>(
    context: Context<'info, DepositAccountConstraints<'info>>,
    usdc_amount: u64,
    minimum_shares: u64,
) -> Result<()> {
    require!(usdc_amount > 0, VaultError::ZeroDeposit);

    let vault_usdc_amount = context.accounts.vault_usdc.amount;
    let total_shares = context.accounts.strategy.total_shares;
    let usdc_decimals = context.accounts.usdc_mint.decimals;
    let manager_key = context.accounts.strategy.manager;
    let strategy_bump = context.accounts.strategy.bump;
    let strategy_key = context.accounts.strategy.key();
    let asset_count = context.accounts.strategy.asset_count as usize;

    let now = Clock::get()?.unix_timestamp;

    // Net asset value over the complete asset set. The assets are exactly indices
    // 0..asset_count, so requiring three accounts per index, in order, each with a
    // matching index, makes it impossible to omit an asset and understate NAV.
    let remaining = context.remaining_accounts;
    require!(
        remaining.len() == asset_count * 3,
        VaultError::IncompleteAssetAccounts
    );

    let mut nav: u128 = vault_usdc_amount as u128;

    for i in 0..asset_count {
        let config_ai = &remaining[i * 3];
        let vault_ai = &remaining[i * 3 + 1];
        let feed_ai = &remaining[i * 3 + 2];

        let config = AssetConfig::load_checked(config_ai)?;
        require_keys_eq!(
            config.strategy,
            strategy_key,
            VaultError::InvalidAssetAccount
        );
        require!(config.index as usize == i, VaultError::InvalidAssetAccount);
        require_keys_eq!(
            vault_ai.key(),
            config.vault,
            VaultError::InvalidAssetAccount
        );

        let price = load_price(feed_ai, &config.price_feed, now)?;
        let amount = read_token_amount(vault_ai)?;
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

    let transfer_accounts = TransferChecked {
        from: context.accounts.depositor_usdc_account.to_account_info(),
        mint: context.accounts.usdc_mint.to_account_info(),
        to: context.accounts.vault_usdc.to_account_info(),
        authority: context.accounts.depositor.to_account_info(),
    };
    let cpi_ctx = CpiContext::new(context.accounts.token_program.key(), transfer_accounts);
    transfer_checked(cpi_ctx, usdc_amount, usdc_decimals)?;

    let signer_seeds: &[&[&[u8]]] = &[&[b"strategy", manager_key.as_ref(), &[strategy_bump]]];
    let mint_accounts = MintTo {
        mint: context.accounts.share_mint.to_account_info(),
        to: context.accounts.depositor_share_account.to_account_info(),
        authority: context.accounts.strategy.to_account_info(),
    };
    let cpi_ctx = CpiContext::new_with_signer(
        context.accounts.token_program.key(),
        mint_accounts,
        signer_seeds,
    );
    mint_to(cpi_ctx, shares_to_mint)?;

    Ok(())
}
