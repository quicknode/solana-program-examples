use anchor_lang::prelude::*;
use anchor_spl::{
    associated_token::AssociatedToken,
    token_interface::{
        burn, transfer_checked, Burn, Mint, TokenAccount, TokenInterface, TransferChecked,
    },
};

use crate::error::FundError;
use crate::oracle::{read_mint_decimals, read_token_mint_and_owner};
use crate::state::{AssetConfig, Fund, MAX_ASSETS};

#[derive(Accounts)]
pub struct WithdrawAccountConstraints<'info> {
    #[account(mut)]
    pub user: Signer<'info>,

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
        associated_token::authority = fund,
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
    require!(shares_to_burn > 0, FundError::ZeroShares);

    let total_shares = context.accounts.fund.total_shares;
    require!(total_shares > 0, FundError::ZeroTotalShares);

    let usdc_decimals = context.accounts.usdc_mint.decimals;
    let fund_index = context.accounts.fund.index;
    let fund_bump = context.accounts.fund.bump;
    let fund_key = context.accounts.fund.key();
    let user_key = context.accounts.user.key();
    let asset_count = context.accounts.fund.asset_count as usize;

    require!(
        context.remaining_accounts.len() == asset_count * 4,
        FundError::IncompleteAssetAccounts
    );

    let shares_u128 = shares_to_burn as u128;
    let total_u128 = total_shares as u128;

    // Every leg is a proportion of the holdings the program has recorded, not of
    // the vault's token balance, so tokens donated into a vault are never paid
    // out. Floored in the fund's favour.
    let proportion = |holding: u64| -> Result<u64> {
        Ok((holding as u128)
            .checked_mul(shares_u128)
            .ok_or(FundError::MathOverflow)?
            .checked_div(total_u128)
            .ok_or(FundError::MathOverflow)? as u64)
    };
    let amount_usdc = proportion(context.accounts.fund.usdc_holdings)?;
    require!(amount_usdc >= min_usdc_out, FundError::UsdcSlippage);
    let mut asset_amounts = [0u64; MAX_ASSETS as usize];
    for (index, amount) in asset_amounts.iter_mut().enumerate().take(asset_count) {
        *amount = proportion(context.accounts.fund.asset_holdings[index])?;
    }

    // Shrink the share supply and the recorded holdings by the withdrawn slice.
    let fund = &mut context.accounts.fund;
    fund.total_shares = total_shares
        .checked_sub(shares_to_burn)
        .ok_or(FundError::MathOverflow)?;
    fund.usdc_holdings = fund
        .usdc_holdings
        .checked_sub(amount_usdc)
        .ok_or(FundError::MathOverflow)?;
    for (index, amount) in asset_amounts.iter().enumerate().take(asset_count) {
        fund.asset_holdings[index] = fund.asset_holdings[index]
            .checked_sub(*amount)
            .ok_or(FundError::MathOverflow)?;
    }

    let index_bytes = fund_index.to_le_bytes();
    let signer_seeds: &[&[&[u8]]] = &[&[b"fund", index_bytes.as_ref(), &[fund_bump]]];

    // Hoist owned account-info handles for every CPI up front, so the asset loop
    // can borrow remaining_accounts without also re-borrowing `context.accounts`
    // (Account is invariant over its lifetime, which otherwise fails to unify).
    let fund_info = context.accounts.fund.to_account_info();
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
            authority: fund_info.clone(),
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
        require_keys_eq!(config.fund, fund_key, FundError::InvalidAssetAccount);
        require!(config.index as usize == i, FundError::InvalidAssetAccount);
        require_keys_eq!(vault_ai.key(), config.vault, FundError::InvalidAssetAccount);
        require_keys_eq!(mint_ai.key(), config.mint, FundError::InvalidAssetAccount);

        let (recipient_mint, recipient_owner) = read_token_mint_and_owner(user_ata_ai)?;
        require_keys_eq!(recipient_owner, user_key, FundError::InvalidRecipient);
        require_keys_eq!(recipient_mint, config.mint, FundError::InvalidRecipient);

        let amount = asset_amounts[i];

        if amount > 0 {
            let decimals = read_mint_decimals(mint_ai)?;
            let transfer_accounts = TransferChecked {
                from: vault_ai.to_account_info(),
                mint: mint_ai.to_account_info(),
                to: user_ata_ai.to_account_info(),
                authority: fund_info.clone(),
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
