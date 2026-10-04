use quasar_lang::cpi::Seed;
use quasar_lang::prelude::*;
use quasar_lang::remaining::RemainingAccounts;
use quasar_spl::prelude::*;

use crate::errors::FundError;
use crate::oracle::{read_mint_decimals, read_token_mint_and_owner};
use crate::state::{
    load_asset_config, snapshot_fund, Fund, ShareMintPda, UsdcVaultPda, FUND_SEED, MAX_ASSETS,
};
use crate::state::{read_asset_holdings, write_asset_holdings};

/// remaining_accounts arrive as, per asset index 0..asset_count:
///   [asset_config, vault, mint, user_token_account]
const ACCOUNTS_PER_ASSET: usize = 4;

#[derive(Accounts)]
pub struct WithdrawAccountConstraints {
    #[account(mut)]
    pub user: Signer,

    #[account(
        mut,
        address = Fund::seeds(fund.index.into()),
        has_one(usdc_mint) @ FundError::InvalidUsdcMint,
    )]
    pub fund: Account<Fund>,

    #[account(mut, address = ShareMintPda::seeds(fund.address()))]
    pub share_mint: InterfaceAccount<Mint>,

    pub usdc_mint: Account<Mint>,

    #[account(mut)]
    pub user_share_account: Account<Token>,

    #[account(mut)]
    pub user_usdc_account: Account<Token>,

    #[account(mut, address = UsdcVaultPda::seeds(fund.address()))]
    pub vault_usdc: InterfaceAccount<Token>,

    pub token_program: Program<TokenProgram>,
    pub system_program: Program<SystemProgram>,
}

fn get_view(remaining: &RemainingAccounts<'_>, index: usize) -> Result<AccountView, ProgramError> {
    let account = remaining
        .get(index)?
        .ok_or(FundError::IncompleteAssetAccounts)?;
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
    require!(shares_to_burn > 0, FundError::ZeroShares);

    let total_shares = u64::from(accounts.fund.total_shares);
    require!(total_shares > 0, FundError::ZeroTotalShares);

    let asset_count = accounts.fund.asset_count as usize;
    require!(
        remaining.get(asset_count * ACCOUNTS_PER_ASSET)?.is_none(),
        FundError::IncompleteAssetAccounts
    );

    let usdc_decimals = accounts.usdc_mint.decimals;
    let fund_index = u64::from(accounts.fund.index);
    let fund_bump = accounts.fund.bump;
    let fund_key = *accounts.fund.address();
    let user_key = *accounts.user.address();

    let shares_u128 = shares_to_burn as u128;
    let total_u128 = total_shares as u128;

    // Every leg is a proportion of the holdings the program has recorded, not of
    // the vault's token balance, so tokens donated into a vault are never paid
    // out. Floored in the fund's favour.
    let proportion = |holding: u64| -> Result<u64, ProgramError> {
        (holding as u128)
            .checked_mul(shares_u128)
            .ok_or(FundError::MathOverflow)?
            .checked_div(total_u128)
            .ok_or(FundError::MathOverflow)?
            .try_into()
            .map_err(|_| FundError::MathOverflow.into())
    };
    let mut fund = snapshot_fund(&accounts.fund);
    let amount_usdc = proportion(fund.usdc_holdings)?;
    require!(amount_usdc >= min_usdc_out, FundError::UsdcSlippage);
    let mut asset_holdings = read_asset_holdings(&fund.asset_holdings);
    let mut asset_amounts = [0u64; MAX_ASSETS as usize];
    for (index, amount) in asset_amounts.iter_mut().enumerate().take(asset_count) {
        *amount = proportion(asset_holdings[index])?;
    }

    // Shrink the share supply and the recorded holdings by the withdrawn slice.
    fund.total_shares = total_shares
        .checked_sub(shares_to_burn)
        .ok_or(FundError::MathOverflow)?;
    fund.usdc_holdings = fund
        .usdc_holdings
        .checked_sub(amount_usdc)
        .ok_or(FundError::MathOverflow)?;
    for (index, amount) in asset_amounts.iter().enumerate().take(asset_count) {
        asset_holdings[index] = asset_holdings[index]
            .checked_sub(*amount)
            .ok_or(FundError::MathOverflow)?;
    }
    fund.asset_holdings = write_asset_holdings(&asset_holdings);
    accounts.fund.set_inner(fund);

    let index_bytes = fund_index.to_le_bytes();
    let bump = [fund_bump];
    let seeds = [
        Seed::from(FUND_SEED),
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

    // USDC payout (fund PDA signs).
    if amount_usdc > 0 {
        accounts
            .token_program
            .transfer_checked(
                &accounts.vault_usdc,
                &accounts.usdc_mint,
                &accounts.user_usdc_account,
                &accounts.fund,
                amount_usdc,
                usdc_decimals,
            )
            .invoke_signed(&seeds)?;
    }

    // Each basket asset, paid in kind, proportional to shares burned.
    for (i, &amount) in asset_amounts.iter().enumerate().take(asset_count) {
        let config_view = get_view(&remaining, i * ACCOUNTS_PER_ASSET)?;
        let vault_view = get_view(&remaining, i * ACCOUNTS_PER_ASSET + 1)?;
        let mint_view = get_view(&remaining, i * ACCOUNTS_PER_ASSET + 2)?;
        let user_ata_view = get_view(&remaining, i * ACCOUNTS_PER_ASSET + 3)?;

        let config = load_asset_config(&config_view)?;
        require_keys_eq!(config.fund, fund_key, FundError::InvalidAssetAccount);
        require!(config.index as usize == i, FundError::InvalidAssetAccount);
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

        let (recipient_mint, recipient_owner) = read_token_mint_and_owner(&user_ata_view)?;
        require_keys_eq!(recipient_owner, user_key, FundError::InvalidRecipient);
        require_keys_eq!(recipient_mint, config.mint, FundError::InvalidRecipient);

        if amount > 0 {
            let decimals = read_mint_decimals(&mint_view)?;
            accounts
                .token_program
                .transfer_checked(
                    &vault_view,
                    &mint_view,
                    &user_ata_view,
                    &accounts.fund,
                    amount,
                    decimals,
                )
                .invoke_signed(&seeds)?;
        }
    }

    Ok(())
}
