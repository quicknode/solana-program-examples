use anchor_lang::prelude::*;
use anchor_spl::token_interface::{
    transfer_checked, Mint, TokenAccount, TokenInterface, TransferChecked,
};

use crate::errors::ErrorCode;
use crate::state::{Market, MarketUser, MARKET_SEED, MARKET_USER_SEED};

pub fn handle_settle_funds(context: Context<SettleFunds>) -> Result<()> {
    let market_user = &mut context.accounts.market_user;
    let market = &context.accounts.market;

    let base_amount = market_user.unsettled_base;
    let quote_amount = market_user.unsettled_quote;

    // Seeds to sign as the market PDA (the authority of both vaults). Built
    // once and reused for the two possible transfers.
    let market_bump = [market.bump];
    let signer_seeds: [&[u8]; 4] = [
        MARKET_SEED,
        market.base_mint.as_ref(),
        market.quote_mint.as_ref(),
        &market_bump,
    ];
    let signer_seeds = &[&signer_seeds[..]];

    if base_amount > 0 {
        transfer_checked(
            CpiContext::new_with_signer(
                context.accounts.token_program.key(),
                TransferChecked {
                    from: context.accounts.base_vault.to_account_info(),
                    mint: context.accounts.base_mint.to_account_info(),
                    to: context.accounts.user_base_account.to_account_info(),
                    authority: market.to_account_info(),
                },
                signer_seeds,
            ),
            base_amount,
            context.accounts.base_mint.decimals,
        )?;
        market_user.unsettled_base = 0;
    }

    if quote_amount > 0 {
        transfer_checked(
            CpiContext::new_with_signer(
                context.accounts.token_program.key(),
                TransferChecked {
                    from: context.accounts.quote_vault.to_account_info(),
                    mint: context.accounts.quote_mint.to_account_info(),
                    to: context.accounts.user_quote_account.to_account_info(),
                    authority: market.to_account_info(),
                },
                signer_seeds,
            ),
            quote_amount,
            context.accounts.quote_mint.decimals,
        )?;
        market_user.unsettled_quote = 0;
    }

    Ok(())
}

#[derive(Accounts)]
pub struct SettleFunds<'info> {
    // `has_one` constraints bind these vaults/mints to the addresses stored
    // on the Market PDA at initialise_market time. Without them a caller
    // could substitute the fee_vault (same mint + same authority as
    // quote_vault) for `quote_vault` and drain accumulated taker fees,
    // since transfer_checked only verifies mint + authority on the source
    // account, not its identity.
    #[account(
        mut,
        has_one = base_vault @ ErrorCode::InvalidBaseVault,
        has_one = quote_vault @ ErrorCode::InvalidQuoteVault,
        has_one = base_mint @ ErrorCode::InvalidBaseMint,
        has_one = quote_mint @ ErrorCode::InvalidQuoteMint,
    )]
    pub market: Account<'info, Market>,

    #[account(
        mut,
        seeds = [MARKET_USER_SEED, market.key().as_ref(), owner.key().as_ref()],
        bump = market_user.bump
    )]
    pub market_user: Account<'info, MarketUser>,

    // Boxed for the same reason as in PlaceOrder —
    // InterfaceAccount is too large to keep on the BPF stack in bulk.
    #[account(mut)]
    pub base_vault: Box<InterfaceAccount<'info, TokenAccount>>,

    #[account(mut)]
    pub quote_vault: Box<InterfaceAccount<'info, TokenAccount>>,

    #[account(mut)]
    pub user_base_account: Box<InterfaceAccount<'info, TokenAccount>>,

    #[account(mut)]
    pub user_quote_account: Box<InterfaceAccount<'info, TokenAccount>>,

    pub base_mint: Box<InterfaceAccount<'info, Mint>>,

    pub quote_mint: Box<InterfaceAccount<'info, Mint>>,

    pub owner: Signer<'info>,

    pub token_program: Interface<'info, TokenInterface>,
}
