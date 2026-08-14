use anchor_lang::prelude::*;
use anchor_spl::token_interface::{
    transfer_checked, Mint, TokenAccount, TokenInterface, TransferChecked,
};

use crate::errors::ErrorCode;
use crate::state::{Market, MarketUser, MARKET_SEED, MARKET_USER_SEED};

pub fn handle_settle_funds(context: &mut Context<SettleFundsAccountConstraints>) -> Result<()> {
    let market_user = &mut context.accounts.market_user;
    let market = &context.accounts.market;

    // Snapshot the amounts the user is owed, then zero the counters
    // BEFORE the token transfers. Checks-effects-interactions: even though
    // Solana CPIs don't reenter in the EVM sense, if either transfer ever
    // gained a path that called back into this program (custom token
    // hooks, transfer-fee extensions with side effects, ...), having stale
    // unsettled_* values readable mid-transfer would let a re-entry double-
    // withdraw. Updating state first makes that class of bug impossible.
    let base_amount = market_user.unsettled_base;
    let quote_amount = market_user.unsettled_quote;
    market_user.unsettled_base = 0;
    market_user.unsettled_quote = 0;

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
                context.accounts.token_program.address(),
                TransferChecked {
                    from: context.accounts.base_vault.cpi_handle_mut(),
                    mint: context.accounts.base_mint.cpi_handle(),
                    to: context.accounts.user_base_account.cpi_handle_mut(),
                    authority: market.cpi_handle(),
                },
                signer_seeds,
            ),
            base_amount,
            context.accounts.base_mint.decimals(),
        )?;
    }

    if quote_amount > 0 {
        transfer_checked(
            CpiContext::new_with_signer(
                context.accounts.token_program.address(),
                TransferChecked {
                    from: context.accounts.quote_vault.cpi_handle_mut(),
                    mint: context.accounts.quote_mint.cpi_handle(),
                    to: context.accounts.user_quote_account.cpi_handle_mut(),
                    authority: market.cpi_handle(),
                },
                signer_seeds,
            ),
            quote_amount,
            context.accounts.quote_mint.decimals(),
        )?;
    }

    Ok(())
}

#[derive(Accounts)]
pub struct SettleFundsAccountConstraints {
    // `address` constraints bind these vaults/mints to the addresses stored
    // on the Market PDA at initialise_market time. Without them a caller
    // could substitute the fee_vault (same mint + same authority as
    // quote_vault) for `quote_vault` and drain accumulated taker fees,
    // since transfer_checked only verifies mint + authority on the source
    // account, not its identity.
    #[account(mut)]
    pub market: BorshAccount<Market>,

    #[account(
        mut,
        seeds = [MARKET_USER_SEED, market.address().as_ref(), owner.address().as_ref()],
        bump = market_user.bump
    )]
    pub market_user: BorshAccount<MarketUser>,

    // Boxed for the same reason as in PlaceOrderAccountConstraints -
    // InterfaceAccount is too large to keep on the BPF stack in bulk.
    #[account(mut, address = market.base_vault @ ErrorCode::InvalidBaseVault)]
    pub base_vault: Box<InterfaceAccount<TokenAccount>>,

    #[account(mut, address = market.quote_vault @ ErrorCode::InvalidQuoteVault)]
    pub quote_vault: Box<InterfaceAccount<TokenAccount>>,

    #[account(mut)]
    pub user_base_account: Box<InterfaceAccount<TokenAccount>>,

    #[account(mut)]
    pub user_quote_account: Box<InterfaceAccount<TokenAccount>>,

    #[account(address = market.base_mint @ ErrorCode::InvalidBaseMint)]
    pub base_mint: Box<InterfaceAccount<Mint>>,

    #[account(address = market.quote_mint @ ErrorCode::InvalidQuoteMint)]
    pub quote_mint: Box<InterfaceAccount<Mint>>,

    pub owner: Signer,

    pub token_program: Interface<'static, TokenInterface>,
}
