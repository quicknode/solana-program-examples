use anchor_lang::prelude::*;
use anchor_spl::{
    associated_token::AssociatedToken,
    token_interface::{
        burn, transfer_checked, Burn, Mint, TokenAccount, TokenInterface, TransferChecked,
    },
};

use crate::error::VaultError;
use crate::oracle::{read_mint_decimals, read_token_amount, read_token_mint_and_owner};
use crate::state::{AssetConfig, Strategy};

#[derive(Accounts)]
pub struct WithdrawAccountConstraints<'info> {
    #[account(mut)]
    pub user: Signer<'info>,

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
        associated_token::mint = share_mint,
        associated_token::authority = user,
        associated_token::token_program = token_program
    )]
    pub user_share_account: Box<InterfaceAccount<'info, TokenAccount>>,

    #[account(
        init_if_needed,
        payer = user,
        associated_token::mint = usdc_mint,
        associated_token::authority = user,
        associated_token::token_program = token_program
    )]
    pub user_usdc_account: Box<InterfaceAccount<'info, TokenAccount>>,

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
    //   [asset_config, vault, mint, user_token_account]
    // The user's asset token accounts must already exist.
}

pub fn handle_withdraw<'info>(
    context: Context<'info, WithdrawAccountConstraints<'info>>,
    shares_to_burn: u64,
    min_usdc_out: u64,
) -> Result<()> {
    require!(shares_to_burn > 0, VaultError::ZeroShares);

    let total_shares = context.accounts.strategy.total_shares;
    require!(total_shares > 0, VaultError::ZeroTotalShares);

    let vault_usdc_amount = context.accounts.vault_usdc.amount;
    let usdc_decimals = context.accounts.usdc_mint.decimals;
    let manager_key = context.accounts.strategy.manager;
    let strategy_bump = context.accounts.strategy.bump;
    let strategy_key = context.accounts.strategy.key();
    let user_key = context.accounts.user.key();
    let asset_count = context.accounts.strategy.asset_count as usize;

    require!(
        context.remaining_accounts.len() == asset_count * 4,
        VaultError::IncompleteAssetAccounts
    );

    let shares_u128 = shares_to_burn as u128;
    let total_u128 = total_shares as u128;

    // USDC leg, floored in the protocol's favour.
    let amount_usdc: u64 = (vault_usdc_amount as u128)
        .checked_mul(shares_u128)
        .ok_or(VaultError::MathOverflow)?
        .checked_div(total_u128)
        .ok_or(VaultError::MathOverflow)? as u64;
    require!(amount_usdc >= min_usdc_out, VaultError::UsdcSlippage);

    // Checks-effects-interactions: shrink supply before any transfer.
    context.accounts.strategy.total_shares = total_shares
        .checked_sub(shares_to_burn)
        .ok_or(VaultError::MathOverflow)?;

    let signer_seeds: &[&[&[u8]]] = &[&[b"strategy", manager_key.as_ref(), &[strategy_bump]]];

    // Hoist owned account-info handles for every CPI up front, so the asset loop
    // can borrow remaining_accounts without also re-borrowing `context.accounts`
    // (Account is invariant over its lifetime, which otherwise fails to unify).
    let strategy_info = context.accounts.strategy.to_account_info();
    let share_mint_info = context.accounts.share_mint.to_account_info();
    let usdc_mint_info = context.accounts.usdc_mint.to_account_info();
    let vault_usdc_info = context.accounts.vault_usdc.to_account_info();
    let user_info = context.accounts.user.to_account_info();
    let user_share_info = context.accounts.user_share_account.to_account_info();
    let user_usdc_info = context.accounts.user_usdc_account.to_account_info();
    let token_program_key = context.accounts.token_program.key();

    // Burn the user's shares.
    let burn_accounts = Burn {
        mint: share_mint_info,
        from: user_share_info,
        authority: user_info,
    };
    burn(
        CpiContext::new(token_program_key, burn_accounts),
        shares_to_burn,
    )?;

    // USDC payout.
    if amount_usdc > 0 {
        let transfer_accounts = TransferChecked {
            from: vault_usdc_info,
            mint: usdc_mint_info,
            to: user_usdc_info,
            authority: strategy_info.clone(),
        };
        transfer_checked(
            CpiContext::new_with_signer(token_program_key, transfer_accounts, signer_seeds),
            amount_usdc,
            usdc_decimals,
        )?;
    }

    // Each basket asset, paid in kind, proportional to shares burned.
    let remaining = context.remaining_accounts;
    for i in 0..asset_count {
        let config_ai = &remaining[i * 4];
        let vault_ai = &remaining[i * 4 + 1];
        let mint_ai = &remaining[i * 4 + 2];
        let user_ata_ai = &remaining[i * 4 + 3];

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
        require_keys_eq!(mint_ai.key(), config.mint, VaultError::InvalidAssetAccount);

        let (recipient_mint, recipient_owner) = read_token_mint_and_owner(user_ata_ai)?;
        require_keys_eq!(recipient_owner, user_key, VaultError::InvalidRecipient);
        require_keys_eq!(recipient_mint, config.mint, VaultError::InvalidRecipient);

        let vault_balance = read_token_amount(vault_ai)?;
        let amount: u64 = (vault_balance as u128)
            .checked_mul(shares_u128)
            .ok_or(VaultError::MathOverflow)?
            .checked_div(total_u128)
            .ok_or(VaultError::MathOverflow)? as u64;

        if amount > 0 {
            let decimals = read_mint_decimals(mint_ai)?;
            let transfer_accounts = TransferChecked {
                from: vault_ai.to_account_info(),
                mint: mint_ai.to_account_info(),
                to: user_ata_ai.to_account_info(),
                authority: strategy_info.clone(),
            };
            transfer_checked(
                CpiContext::new_with_signer(token_program_key, transfer_accounts, signer_seeds),
                amount,
                decimals,
            )?;
        }
    }

    Ok(())
}
