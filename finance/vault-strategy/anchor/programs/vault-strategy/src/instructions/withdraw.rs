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

pub fn handle_withdraw(
    context: &mut Context<WithdrawAccountConstraints>,
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
    let strategy_key = *context.accounts.strategy.address();
    let user_key = *context.accounts.user.address();
    let asset_count = context.accounts.strategy.asset_count as usize;

    require!(
        context.remaining_accounts()?.len() == asset_count * 4,
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

    // `remaining_accounts()` takes `&mut Context`, so collect it before the
    // per-account views below borrow `context.accounts`.
    let remaining = context.remaining_accounts()?;

    // Hoist owned account-info handles for every CPI up front, so the asset loop
    // can borrow remaining_accounts without also re-borrowing `context.accounts`.
    // `strategy` signs the payouts below. It is a data account holding a live
    // borrow on its buffer, so release it across the CPIs and take it back
    // afterwards — the runtime rejects a CPI that borrows an account we hold.
    context.accounts.strategy.release_borrow()?;

    // `AccountView` is Copy; each copy still points at the same account, and the
    // typed handles below carry the borrow provenance v2 wants.
    let strategy_view = *context.accounts.strategy.account();
    let mut share_mint_view = *context.accounts.share_mint.account();
    let usdc_mint_view = *context.accounts.usdc_mint.account();
    let mut vault_usdc_view = *context.accounts.vault_usdc.account();
    let user_view = *context.accounts.user.account();
    let mut user_share_view = *context.accounts.user_share_account.account();
    let mut user_usdc_view = *context.accounts.user_usdc_account.account();
    let token_program_key = context.accounts.token_program.address();

    // Burn the user's shares.
    let burn_accounts = Burn {
        mint: CpiHandleMut::writable(&mut share_mint_view),
        from: CpiHandleMut::writable(&mut user_share_view),
        authority: CpiHandle::readonly(&user_view),
    };
    burn(
        CpiContext::new(token_program_key, burn_accounts),
        shares_to_burn,
    )?;

    // USDC payout.
    if amount_usdc > 0 {
        let transfer_accounts = TransferChecked {
            from: CpiHandleMut::writable(&mut vault_usdc_view),
            mint: CpiHandle::readonly(&usdc_mint_view),
            to: CpiHandleMut::writable(&mut user_usdc_view),
            authority: CpiHandle::readonly(&strategy_view),
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

        let config = AssetConfig::load_checked(&config_ai)?;
        require_keys_eq!(
            config.strategy,
            strategy_key,
            VaultError::InvalidAssetAccount
        );
        require!(config.index as usize == i, VaultError::InvalidAssetAccount);
        require_keys_eq!(
            *vault_ai.address(),
            config.vault,
            VaultError::InvalidAssetAccount
        );
        require_keys_eq!(
            *mint_ai.address(),
            config.mint,
            VaultError::InvalidAssetAccount
        );

        let (recipient_mint, recipient_owner) = read_token_mint_and_owner(&user_ata_ai)?;
        require_keys_eq!(recipient_owner, user_key, VaultError::InvalidRecipient);
        require_keys_eq!(recipient_mint, config.mint, VaultError::InvalidRecipient);

        let vault_balance = read_token_amount(&vault_ai)?;
        let amount: u64 = (vault_balance as u128)
            .checked_mul(shares_u128)
            .ok_or(VaultError::MathOverflow)?
            .checked_div(total_u128)
            .ok_or(VaultError::MathOverflow)? as u64;

        if amount > 0 {
            let decimals = read_mint_decimals(&mint_ai)?;
            let transfer_accounts = TransferChecked {
                from: CpiHandleMut::writable(&mut vault_ai),
                mint: CpiHandle::readonly(&mint_ai),
                to: CpiHandleMut::writable(&mut user_ata_ai),
                authority: CpiHandle::readonly(&strategy_view),
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
