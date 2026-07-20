use quasar_lang::prelude::*;
use quasar_lang::remaining::RemainingAccounts;
use quasar_spl::prelude::*;

use crate::errors::VaultError;
use crate::oracle::{read_mint_decimals, read_token_amount, read_token_mint_and_owner};
use crate::state::{
    load_asset_config, snapshot_strategy, ShareMintPda, Strategy, UsdcVaultPda, STRATEGY_SEED,
};

/// remaining_accounts arrive as, per asset index 0..asset_count:
///   [asset_config, vault, mint, user_token_account]
const ACCOUNTS_PER_ASSET: usize = 4;

#[derive(Accounts)]
pub struct WithdrawAccountConstraints {
    #[account(mut)]
    pub user: Signer,

    #[account(
        mut,
        address = Strategy::seeds(strategy.index.into()),
        has_one(usdc_mint) @ VaultError::InvalidUsdcMint,
    )]
    pub strategy: Account<Strategy>,

    #[account(mut, address = ShareMintPda::seeds(strategy.address()))]
    pub share_mint: InterfaceAccount<Mint>,

    pub usdc_mint: Account<Mint>,

    #[account(mut)]
    pub user_share_account: Account<Token>,

    #[account(mut)]
    pub user_usdc_account: Account<Token>,

    #[account(mut, address = UsdcVaultPda::seeds(strategy.address()))]
    pub vault_usdc: InterfaceAccount<Token>,

    pub token_program: Program<TokenProgram>,
    pub system_program: Program<SystemProgram>,
}

fn get_view(remaining: &RemainingAccounts<'_>, index: usize) -> Result<AccountView, ProgramError> {
    let account = remaining
        .get(index)?
        .ok_or(VaultError::IncompleteAssetAccounts)?;
    // SAFETY: read-only forwarding; no mutable alias taken across these views.
    Ok(unsafe { account.as_account_view_unchecked() }.clone())
}

#[inline(always)]
pub fn handle_withdraw(
    accounts: &mut WithdrawAccountConstraints,
    remaining: RemainingAccounts<'_>,
    shares_to_burn: u64,
    min_usdc_out: u64,
) -> Result<(), ProgramError> {
    require!(shares_to_burn > 0, VaultError::ZeroShares);

    let total_shares = u64::from(accounts.strategy.total_shares);
    require!(total_shares > 0, VaultError::ZeroTotalShares);

    let asset_count = accounts.strategy.asset_count as usize;
    require!(
        remaining.get(asset_count * ACCOUNTS_PER_ASSET)?.is_none(),
        VaultError::IncompleteAssetAccounts
    );

    let vault_usdc_amount = accounts.vault_usdc.amount();
    let usdc_decimals = accounts.usdc_mint.decimals;
    let strategy_index = u64::from(accounts.strategy.index);
    let strategy_bump = accounts.strategy.bump;
    let strategy_key = *accounts.strategy.address();
    let user_key = *accounts.user.address();

    let shares_u128 = shares_to_burn as u128;
    let total_u128 = total_shares as u128;

    // USDC leg, floored in the protocol's favour.
    let amount_usdc: u64 = (vault_usdc_amount as u128)
        .checked_mul(shares_u128)
        .ok_or(VaultError::MathOverflow)?
        .checked_div(total_u128)
        .ok_or(VaultError::MathOverflow)?
        .try_into()
        .map_err(|_| VaultError::MathOverflow)?;
    require!(amount_usdc >= min_usdc_out, VaultError::UsdcSlippage);

    // Checks-effects-interactions: shrink supply before any transfer.
    let mut strategy = snapshot_strategy(&accounts.strategy);
    strategy.total_shares = total_shares
        .checked_sub(shares_to_burn)
        .ok_or(VaultError::MathOverflow)?;
    accounts.strategy.set_inner(strategy);

    let index_bytes = strategy_index.to_le_bytes();
    let bump = [strategy_bump];
    let seeds = [
        Seed::from(STRATEGY_SEED),
        Seed::from(index_bytes.as_ref()),
        Seed::from(bump.as_ref()),
    ];

    // Burn the user's shares (user signs).
    accounts
        .token_program
        .burn(
            &accounts.user_share_account,
            &accounts.share_mint,
            &accounts.user,
            shares_to_burn,
        )
        .invoke()?;

    // USDC payout (strategy PDA signs).
    if amount_usdc > 0 {
        accounts
            .token_program
            .transfer_checked(
                &accounts.vault_usdc,
                &accounts.usdc_mint,
                &accounts.user_usdc_account,
                &accounts.strategy,
                amount_usdc,
                usdc_decimals,
            )
            .invoke_signed(&seeds)?;
    }

    // Each basket asset, paid in kind, proportional to shares burned.
    for i in 0..asset_count {
        let config_view = get_view(&remaining, i * ACCOUNTS_PER_ASSET)?;
        let vault_view = get_view(&remaining, i * ACCOUNTS_PER_ASSET + 1)?;
        let mint_view = get_view(&remaining, i * ACCOUNTS_PER_ASSET + 2)?;
        let user_ata_view = get_view(&remaining, i * ACCOUNTS_PER_ASSET + 3)?;

        let config = load_asset_config(&config_view)?;
        require_keys_eq!(
            config.strategy,
            strategy_key,
            VaultError::InvalidAssetAccount
        );
        require!(config.index as usize == i, VaultError::InvalidAssetAccount);
        require_keys_eq!(
            *vault_view.address(),
            config.vault,
            VaultError::InvalidAssetAccount
        );
        require_keys_eq!(
            *mint_view.address(),
            config.mint,
            VaultError::InvalidAssetAccount
        );

        let (recipient_mint, recipient_owner) = read_token_mint_and_owner(&user_ata_view)?;
        require_keys_eq!(recipient_owner, user_key, VaultError::InvalidRecipient);
        require_keys_eq!(recipient_mint, config.mint, VaultError::InvalidRecipient);

        let vault_balance = read_token_amount(&vault_view)?;
        let amount: u64 = (vault_balance as u128)
            .checked_mul(shares_u128)
            .ok_or(VaultError::MathOverflow)?
            .checked_div(total_u128)
            .ok_or(VaultError::MathOverflow)?
            .try_into()
            .map_err(|_| VaultError::MathOverflow)?;

        if amount > 0 {
            let decimals = read_mint_decimals(&mint_view)?;
            accounts
                .token_program
                .transfer_checked(
                    &vault_view,
                    &mint_view,
                    &user_ata_view,
                    &accounts.strategy,
                    amount,
                    decimals,
                )
                .invoke_signed(&seeds)?;
        }
    }

    Ok(())
}
