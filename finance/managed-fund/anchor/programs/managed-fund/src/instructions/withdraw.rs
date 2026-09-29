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
pub struct WithdrawAccountConstraints {
    #[account(mut)]
    pub user: Signer,

    #[account(
        mut,
        seeds = [b"fund", fund.index.to_le_bytes()],
        bump = fund.bump,
    )]
    pub fund: Box<BorshAccount<Fund>>,

    #[account(
        mut,
        seeds = [b"share_mint", fund.address().as_ref()],
        bump
    )]
    pub share_mint: Box<InterfaceAccount<Mint>>,

    #[account(address = fund.usdc_mint @ FundError::InvalidUsdcMint)]
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
        associated_token::authority = fund,
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

pub fn handle_withdraw(
    context: &mut Context<WithdrawAccountConstraints>,
    shares_to_burn: u64,
    min_usdc_out: u64,
) -> Result<()> {
    require!(shares_to_burn > 0, FundError::ZeroShares);

    let total_shares = context.accounts.fund.total_shares;
    require!(total_shares > 0, FundError::ZeroTotalShares);

    let usdc_decimals = context.accounts.usdc_mint.decimals();
    let fund_index = context.accounts.fund.index;
    let fund_bump = context.accounts.fund.bump;
    let fund_key = *context.accounts.fund.address();
    let user_key = *context.accounts.user.address();
    let asset_count = context.accounts.fund.asset_count as usize;

    require!(
        context.remaining_accounts()?.len() == asset_count * 4,
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

    // Checks-effects-interactions: shrink supply and holdings before any transfer.
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

    // `remaining_accounts()` takes `&mut Context`, so collect it before the
    // per-account views below borrow `context.accounts`.
    let remaining = context.remaining_accounts()?;

    // `fund` signs the payouts below. It is a data account holding a live
    // borrow on its buffer, so release it across the CPIs and take it back
    // afterwards: the runtime rejects a CPI that borrows an account we hold.
    context.accounts.fund.release_borrow()?;

    // Every other account here goes through its own wrapper handle. A handle
    // built by hand over a copy of the `AccountView` keeps the runtime borrow
    // check on, and a mutable data account is marked exclusively borrowed, so
    // the copy would be rejected where the wrapper's handle is not.
    let fund_view = *context.accounts.fund.account();
    let token_program_key = context.accounts.token_program.address();

    // Burn the user's shares.
    let burn_accounts = Burn {
        mint: context.accounts.share_mint.to_cpi_handle_mut(),
        from: context.accounts.user_share_account.to_cpi_handle_mut(),
        authority: context.accounts.user.cpi_handle(),
    };
    burn(
        CpiContext::new(token_program_key, burn_accounts),
        shares_to_burn,
    )?;

    // USDC payout.
    if amount_usdc > 0 {
        let transfer_accounts = TransferChecked {
            from: context.accounts.vault_usdc.to_cpi_handle_mut(),
            mint: context.accounts.usdc_mint.to_cpi_handle(),
            to: context.accounts.user_usdc_account.to_cpi_handle_mut(),
            authority: CpiHandle::readonly(&fund_view),
        };
        transfer_checked(
            CpiContext::new_with_signer(token_program_key, transfer_accounts, signer_seeds),
            amount_usdc,
            usdc_decimals,
        )?;
    }

    // Each basket asset, paid in kind, proportional to shares burned.
    for i in 0..asset_count {
        let config_ai = &remaining[i * 4];
        let mut vault_ai = remaining[i * 4 + 1];
        let mint_ai = remaining[i * 4 + 2];
        let mut user_ata_ai = remaining[i * 4 + 3];

        let config = AssetConfig::load_checked(config_ai)?;
        require_keys_eq!(config.fund, fund_key, FundError::InvalidAssetAccount);
        require!(config.index as usize == i, FundError::InvalidAssetAccount);
        require_keys_eq!(
            *vault_ai.address(),
            config.vault,
            FundError::InvalidAssetAccount
        );
        require_keys_eq!(
            *mint_ai.address(),
            config.mint,
            FundError::InvalidAssetAccount
        );

        let (recipient_mint, recipient_owner) = read_token_mint_and_owner(&user_ata_ai)?;
        require_keys_eq!(recipient_owner, user_key, FundError::InvalidRecipient);
        require_keys_eq!(recipient_mint, config.mint, FundError::InvalidRecipient);

        let amount = asset_amounts[i];

        if amount > 0 {
            let decimals = read_mint_decimals(&mint_ai)?;
            let transfer_accounts = TransferChecked {
                from: CpiHandleMut::writable(&mut vault_ai),
                mint: CpiHandle::readonly(&mint_ai),
                to: CpiHandleMut::writable(&mut user_ata_ai),
                authority: CpiHandle::readonly(&fund_view),
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
