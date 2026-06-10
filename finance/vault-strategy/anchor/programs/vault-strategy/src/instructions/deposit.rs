use anchor_lang::prelude::*;
use anchor_spl::{
    associated_token::AssociatedToken,
    token_interface::{
        mint_to, transfer_checked, Mint, MintTo, TokenAccount, TokenInterface, TransferChecked,
    },
};

use crate::error::VaultError;
use crate::state::Strategy;

/// Byte offset of `price` (i64) inside a PriceUpdateV2 account data:
///   8 discriminator + 32 write_authority + 1 verification_level + 32 feed_id = 73
const PYTH_PRICE_OFFSET: usize = 73;
/// Byte offset of `publish_time` (i64):
///   price(8) + conf(8) + exponent(4) = +20 bytes after price
const PYTH_PUBLISH_TIME_OFFSET: usize = PYTH_PRICE_OFFSET + 8 + 8 + 4; // 93

fn read_pyth_price(account_data: &[u8]) -> Result<(i64, i64)> {
    if account_data.len() < PYTH_PUBLISH_TIME_OFFSET + 8 {
        return err!(VaultError::InvalidPriceFeed);
    }
    let price = i64::from_le_bytes(
        account_data[PYTH_PRICE_OFFSET..PYTH_PRICE_OFFSET + 8]
            .try_into()
            .map_err(|_| VaultError::InvalidPriceFeed)?,
    );
    let publish_time = i64::from_le_bytes(
        account_data[PYTH_PUBLISH_TIME_OFFSET..PYTH_PUBLISH_TIME_OFFSET + 8]
            .try_into()
            .map_err(|_| VaultError::InvalidPriceFeed)?,
    );
    Ok((price, publish_time))
}

#[derive(Accounts)]
pub struct DepositAccountConstraints<'info> {
    #[account(mut)]
    pub depositor: Signer<'info>,

    #[account(
        mut,
        has_one = usdc_mint @ VaultError::InvalidUsdcMint,
        has_one = asset_mint_a @ VaultError::InvalidAssetMint,
        has_one = asset_mint_b @ VaultError::InvalidAssetMint,
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

    pub asset_mint_a: Box<InterfaceAccount<'info, Mint>>,

    pub asset_mint_b: Box<InterfaceAccount<'info, Mint>>,

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

    #[account(
        associated_token::mint = asset_mint_a,
        associated_token::authority = strategy,
        associated_token::token_program = token_program
    )]
    pub vault_asset_a: Box<InterfaceAccount<'info, TokenAccount>>,

    #[account(
        associated_token::mint = asset_mint_b,
        associated_token::authority = strategy,
        associated_token::token_program = token_program
    )]
    pub vault_asset_b: Box<InterfaceAccount<'info, TokenAccount>>,

    /// CHECK: Pyth PriceUpdateV2 for asset_a - key validated against strategy.price_feed_a
    #[account(
        constraint = price_feed_a.key() == strategy.price_feed_a @ VaultError::InvalidPriceFeed
    )]
    pub price_feed_a: UncheckedAccount<'info>,

    /// CHECK: Pyth PriceUpdateV2 for asset_b - key validated against strategy.price_feed_b
    #[account(
        constraint = price_feed_b.key() == strategy.price_feed_b @ VaultError::InvalidPriceFeed
    )]
    pub price_feed_b: UncheckedAccount<'info>,

    pub associated_token_program: Program<'info, AssociatedToken>,
    pub token_program: Interface<'info, TokenInterface>,
    pub system_program: Program<'info, System>,
}

pub fn handle_deposit(
    context: Context<DepositAccountConstraints>,
    usdc_amount: u64,
    minimum_shares: u64,
) -> Result<()> {
    require!(usdc_amount > 0, VaultError::ZeroDeposit);

    // Snapshot all values needed before any mutable borrow
    let vault_usdc_amount = context.accounts.vault_usdc.amount;
    let vault_asset_a_amount = context.accounts.vault_asset_a.amount;
    let vault_asset_b_amount = context.accounts.vault_asset_b.amount;
    let total_shares = context.accounts.strategy.total_shares;
    let usdc_decimals = context.accounts.usdc_mint.decimals;
    let manager_key = context.accounts.strategy.manager;
    let strategy_bump = context.accounts.strategy.bump;

    // Read Pyth prices from raw account data (avoids borsh version incompatibility)
    let price_feed_a_data = context.accounts.price_feed_a.try_borrow_data()?;
    let (price_a, publish_time_a) = read_pyth_price(&price_feed_a_data)?;

    let price_feed_b_data = context.accounts.price_feed_b.try_borrow_data()?;
    let (price_b, publish_time_b) = read_pyth_price(&price_feed_b_data)?;

    require!(price_a > 0, VaultError::NegativePrice);
    require!(price_b > 0, VaultError::NegativePrice);

    // Pyth price accounts expose publish_time as unix timestamp rather than slot.
    // We accept prices up to MAX_PRICE_AGE_SECONDS old.
    const MAX_PRICE_AGE_SECONDS: i64 = 60;
    let clock = Clock::get()?;
    require!(
        clock
            .unix_timestamp
            .checked_sub(publish_time_a)
            .ok_or(VaultError::MathOverflow)?
            <= MAX_PRICE_AGE_SECONDS,
        VaultError::StalePriceFeed
    );
    require!(
        clock
            .unix_timestamp
            .checked_sub(publish_time_b)
            .ok_or(VaultError::MathOverflow)?
            <= MAX_PRICE_AGE_SECONDS,
        VaultError::StalePriceFeed
    );

    // Pyth USD pairs use exponent -8 (price * 10^-8 = dollars per token).
    // With both USDC and basket tokens at 6 decimals, usdc_base_per_token_base
    // = price_dollars * 10^(usdc_decimals - token_decimals) = price_dollars * 1
    // = price * 10^(-8). Integer form (multiply before divide):
    //   asset_value = vault_balance * price / 10^8
    const PYTH_PRICE_PRECISION: u128 = 100_000_000; // 10^8

    // Compute NAV: vault_usdc + vault_asset_a * price_a / 10^8 + vault_asset_b * price_b / 10^8
    let usdc_value = vault_usdc_amount as u128;

    let asset_a_value = (vault_asset_a_amount as u128)
        .checked_mul(price_a as u128)
        .ok_or(VaultError::MathOverflow)?
        .checked_div(PYTH_PRICE_PRECISION)
        .ok_or(VaultError::MathOverflow)?;

    let asset_b_value = (vault_asset_b_amount as u128)
        .checked_mul(price_b as u128)
        .ok_or(VaultError::MathOverflow)?
        .checked_div(PYTH_PRICE_PRECISION)
        .ok_or(VaultError::MathOverflow)?;

    let nav = usdc_value
        .checked_add(asset_a_value)
        .ok_or(VaultError::MathOverflow)?
        .checked_add(asset_b_value)
        .ok_or(VaultError::MathOverflow)?;

    // shares = usdc_amount * total_shares / nav  (floor)
    // first deposit (total_shares == 0): 1:1
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

    // Checks-effects-interactions: update state before CPIs
    context.accounts.strategy.total_shares = context
        .accounts
        .strategy
        .total_shares
        .checked_add(shares_to_mint)
        .ok_or(VaultError::MathOverflow)?;

    // Transfer USDC from depositor to vault
    let transfer_accounts = TransferChecked {
        from: context.accounts.depositor_usdc_account.to_account_info(),
        mint: context.accounts.usdc_mint.to_account_info(),
        to: context.accounts.vault_usdc.to_account_info(),
        authority: context.accounts.depositor.to_account_info(),
    };
    let cpi_ctx = CpiContext::new(context.accounts.token_program.key(), transfer_accounts);
    transfer_checked(cpi_ctx, usdc_amount, usdc_decimals)?;

    // Mint shares to depositor - strategy PDA signs
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
