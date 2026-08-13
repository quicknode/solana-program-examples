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
pub struct WithdrawAccountConstraints {
    #[account(mut)]
    pub user: Signer,

    #[account(
        mut,
        has_one = usdc_mint @ VaultError::InvalidUsdcMint,
        seeds = [b"strategy", strategy.index.to_le_bytes().as_ref()],
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
        associated_token::mint = share_mint,
        associated_token::authority = user,
        associated_token::token_program = token_program
    )]
    pub user_share_account: Box<InterfaceAccount<TokenAccount>>,

    #[account(
        init_if_needed,
        payer = user,
        associated_token::mint = usdc_mint,
        associated_token::authority = user,
        associated_token::token_program = token_program
    )]
    pub user_usdc_account: Box<InterfaceAccount<TokenAccount>>,

    #[account(
        mut,
        associated_token::mint = usdc_mint,
        associated_token::authority = strategy,
        associated_token::token_program = token_program
    )]
    pub vault_usdc: Box<InterfaceAccount<TokenAccount>>,

    pub associated_token_program: Program<AssociatedToken>,
    pub token_program: Interface<'static, TokenInterface>,
    pub system_program: Program<System>,
    // remaining_accounts: for each asset index 0..asset_count, in order:
    //   [asset_config, vault, mint, user_token_account]
    // The user's asset token accounts must already exist.
}

pub fn handle_withdraw<'info>(
    context: &mut Context<'info, WithdrawAccountConstraints<'info>>,
    shares_to_burn: u64,
    min_usdc_out: u64,
) -> Result<()> {
    require!(shares_to_burn > 0, VaultError::ZeroShares);

    let total_shares = context.accounts.strategy.total_shares;
    require!(total_shares > 0, VaultError::ZeroTotalShares);

    let vault_usdc_amount = context.accounts.vault_usdc.amount();
    let usdc_decimals = context.accounts.usdc_mint.decimals();
    let strategy_index = context.accounts.strategy.index;
    let strategy_bump = context.accounts.strategy.bump;
    let strategy_key = context.accounts.strategy.address();
    let user_key = context.accounts.user.address();
    let asset_count = context.accounts.strategy.asset_count as usize;

    require!(
        context.remaining_accounts().len() == asset_count * 4,
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

    let index_bytes = strategy_index.to_le_bytes();
    let signer_seeds: &[&[&[u8]]] = &[&[b"strategy", index_bytes.as_ref(), &[strategy_bump]]];

    // Hoist owned account-info handles for every CPI up front, so the asset loop
    // can borrow remaining_accounts without also re-borrowing `context.accounts`
    // (Account is invariant over its lifetime, which otherwise fails to unify).
    let strategy_info = context.accounts.strategy.cpi_handle_mut();
    let share_mint_info = context.accounts.share_mint.cpi_handle_mut();
    let usdc_mint_info = context.accounts.usdc_mint.cpi_handle_mut();
    let vault_usdc_info = context.accounts.vault_usdc.cpi_handle_mut();
    let user_info = context.accounts.user.cpi_handle_mut();
    let user_share_info = context.accounts.user_share_account.cpi_handle_mut();
    let user_usdc_info = context.accounts.user_usdc_account.cpi_handle_mut();
    let token_program_key = context.accounts.token_program.address();

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
    let remaining = context.remaining_accounts();
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
            vault_ai.address(),
            config.vault,
            VaultError::InvalidAssetAccount
        );
        require_keys_eq!(mint_ai.address(), config.mint, VaultError::InvalidAssetAccount);

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
                from: vault_ai.cpi_handle_mut(),
                mint: mint_ai.cpi_handle_mut(),
                to: user_ata_ai.cpi_handle_mut(),
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
