use anchor_lang::prelude::*;
use anchor_spl::{
    associated_token::AssociatedToken,
    token_interface::{
        burn, transfer_checked, Burn, Mint, TokenAccount, TokenInterface, TransferChecked,
    },
};

use crate::error::VaultError;
use crate::state::Strategy;

#[derive(Accounts)]
pub struct WithdrawAccountConstraints<'info> {
    #[account(mut)]
    pub user: Signer<'info>,

    #[account(
        mut,
        seeds = [b"strategy", strategy.manager.as_ref()],
        bump = strategy.bump
    )]
    pub strategy: Account<'info, Strategy>,

    #[account(
        mut,
        seeds = [b"share_mint", strategy.key().as_ref()],
        bump
    )]
    pub share_mint: InterfaceAccount<'info, Mint>,

    pub usdc_mint: InterfaceAccount<'info, Mint>,

    pub asset_mint_a: InterfaceAccount<'info, Mint>,

    pub asset_mint_b: InterfaceAccount<'info, Mint>,

    #[account(
        mut,
        associated_token::mint = share_mint,
        associated_token::authority = user,
        associated_token::token_program = token_program
    )]
    pub user_share_account: InterfaceAccount<'info, TokenAccount>,

    #[account(
        init_if_needed,
        payer = user,
        associated_token::mint = usdc_mint,
        associated_token::authority = user,
        associated_token::token_program = token_program
    )]
    pub user_usdc_account: InterfaceAccount<'info, TokenAccount>,

    #[account(
        init_if_needed,
        payer = user,
        associated_token::mint = asset_mint_a,
        associated_token::authority = user,
        associated_token::token_program = token_program
    )]
    pub user_asset_a_account: InterfaceAccount<'info, TokenAccount>,

    #[account(
        init_if_needed,
        payer = user,
        associated_token::mint = asset_mint_b,
        associated_token::authority = user,
        associated_token::token_program = token_program
    )]
    pub user_asset_b_account: InterfaceAccount<'info, TokenAccount>,

    #[account(
        mut,
        associated_token::mint = usdc_mint,
        associated_token::authority = strategy,
        associated_token::token_program = token_program
    )]
    pub vault_usdc: InterfaceAccount<'info, TokenAccount>,

    #[account(
        mut,
        associated_token::mint = asset_mint_a,
        associated_token::authority = strategy,
        associated_token::token_program = token_program
    )]
    pub vault_asset_a: InterfaceAccount<'info, TokenAccount>,

    #[account(
        mut,
        associated_token::mint = asset_mint_b,
        associated_token::authority = strategy,
        associated_token::token_program = token_program
    )]
    pub vault_asset_b: InterfaceAccount<'info, TokenAccount>,

    pub associated_token_program: Program<'info, AssociatedToken>,
    pub token_program: Interface<'info, TokenInterface>,
    pub system_program: Program<'info, System>,
}

pub fn handle_withdraw(
    context: Context<WithdrawAccountConstraints>,
    shares_to_burn: u64,
    min_usdc_out: u64,
    min_asset_a_out: u64,
    min_asset_b_out: u64,
) -> Result<()> {
    require!(shares_to_burn > 0, VaultError::ZeroShares);

    let total_shares = context.accounts.strategy.total_shares;
    require!(total_shares > 0, VaultError::ZeroTotalShares);

    // Snapshot values before any state mutation
    let vault_usdc_amount = context.accounts.vault_usdc.amount;
    let vault_asset_a_amount = context.accounts.vault_asset_a.amount;
    let vault_asset_b_amount = context.accounts.vault_asset_b.amount;
    let usdc_decimals = context.accounts.usdc_mint.decimals;
    let asset_a_decimals = context.accounts.asset_mint_a.decimals;
    let asset_b_decimals = context.accounts.asset_mint_b.decimals;
    let manager_key = context.accounts.strategy.manager;
    let strategy_bump = context.accounts.strategy.bump;

    let shares_u128 = shares_to_burn as u128;
    let total_u128 = total_shares as u128;

    // Proportional amounts — floor division (user gets floor)
    let amount_usdc: u64 = (vault_usdc_amount as u128)
        .checked_mul(shares_u128)
        .ok_or(VaultError::MathOverflow)?
        .checked_div(total_u128)
        .ok_or(VaultError::MathOverflow)? as u64;

    let amount_a: u64 = (vault_asset_a_amount as u128)
        .checked_mul(shares_u128)
        .ok_or(VaultError::MathOverflow)?
        .checked_div(total_u128)
        .ok_or(VaultError::MathOverflow)? as u64;

    let amount_b: u64 = (vault_asset_b_amount as u128)
        .checked_mul(shares_u128)
        .ok_or(VaultError::MathOverflow)?
        .checked_div(total_u128)
        .ok_or(VaultError::MathOverflow)? as u64;

    require!(amount_usdc >= min_usdc_out, VaultError::UsdcSlippage);
    require!(amount_a >= min_asset_a_out, VaultError::AssetASlippage);
    require!(amount_b >= min_asset_b_out, VaultError::AssetBSlippage);

    // Checks-effects-interactions: update total_shares before any CPIs
    context.accounts.strategy.total_shares = total_shares
        .checked_sub(shares_to_burn)
        .ok_or(VaultError::MathOverflow)?;

    let signer_seeds: &[&[&[u8]]] = &[&[b"strategy", manager_key.as_ref(), &[strategy_bump]]];

    // Burn shares (user is signer authority)
    let burn_accounts = Burn {
        mint: context.accounts.share_mint.to_account_info(),
        from: context.accounts.user_share_account.to_account_info(),
        authority: context.accounts.user.to_account_info(),
    };
    let cpi_ctx = CpiContext::new(context.accounts.token_program.key(), burn_accounts);
    burn(cpi_ctx, shares_to_burn)?;

    // Transfer USDC from vault to user
    if amount_usdc > 0 {
        let transfer_accounts = TransferChecked {
            from: context.accounts.vault_usdc.to_account_info(),
            mint: context.accounts.usdc_mint.to_account_info(),
            to: context.accounts.user_usdc_account.to_account_info(),
            authority: context.accounts.strategy.to_account_info(),
        };
        let cpi_ctx = CpiContext::new_with_signer(
            context.accounts.token_program.key(),
            transfer_accounts,
            signer_seeds,
        );
        transfer_checked(cpi_ctx, amount_usdc, usdc_decimals)?;
    }

    // Transfer asset_a from vault to user
    if amount_a > 0 {
        let transfer_accounts = TransferChecked {
            from: context.accounts.vault_asset_a.to_account_info(),
            mint: context.accounts.asset_mint_a.to_account_info(),
            to: context.accounts.user_asset_a_account.to_account_info(),
            authority: context.accounts.strategy.to_account_info(),
        };
        let cpi_ctx = CpiContext::new_with_signer(
            context.accounts.token_program.key(),
            transfer_accounts,
            signer_seeds,
        );
        transfer_checked(cpi_ctx, amount_a, asset_a_decimals)?;
    }

    // Transfer asset_b from vault to user
    if amount_b > 0 {
        let transfer_accounts = TransferChecked {
            from: context.accounts.vault_asset_b.to_account_info(),
            mint: context.accounts.asset_mint_b.to_account_info(),
            to: context.accounts.user_asset_b_account.to_account_info(),
            authority: context.accounts.strategy.to_account_info(),
        };
        let cpi_ctx = CpiContext::new_with_signer(
            context.accounts.token_program.key(),
            transfer_accounts,
            signer_seeds,
        );
        transfer_checked(cpi_ctx, amount_b, asset_b_decimals)?;
    }

    Ok(())
}
