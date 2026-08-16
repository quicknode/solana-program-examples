use anchor_lang::prelude::*;
use anchor_spl::token_interface::{
    transfer_checked, Mint, TokenAccount, TokenInterface, TransferChecked,
};

use crate::errors::ErrorCode;
use crate::state::{Market, MARKET_SEED};

/// Drain the market's accumulated taker fees into the authority's token
/// account. Authority-only - arbitrary callers must not be able to siphon
/// the fee vault. Transfers the current balance of the fee vault in full;
/// a partial-withdraw flavour could take an amount parameter, left out here
/// to keep the example focused.
pub fn handle_withdraw_fees(context: &mut Context<WithdrawFeesAccountConstraints>) -> Result<()> {
    let market = &context.accounts.market;

    require!(
        *context.accounts.authority.address() == market.authority,
        ErrorCode::NotMarketAuthority
    );

    let fee_balance = context.accounts.fee_vault.amount();
    if fee_balance == 0 {
        // Nothing to do - exit quietly rather than failing, so this
        // instruction is safe to call on a cron/heartbeat even when there
        // haven't been any fills since the last run.
        return Ok(());
    }

    // Copied out because `market` has to release its data borrow before it
    // signs: the runtime would otherwise reject the CPI's own borrow of the
    // same account with AccountBorrowFailed.
    let market_bump = [market.bump];
    let base_mint = market.base_mint;
    let quote_mint = market.quote_mint;
    let signer_seeds: [&[u8]; 4] = [
        MARKET_SEED,
        base_mint.as_ref(),
        quote_mint.as_ref(),
        &market_bump,
    ];
    let signer_seeds = &[&signer_seeds[..]];
    let quote_decimals = context.accounts.quote_mint.decimals();

    context.accounts.market.release_borrow()?;
    transfer_checked(
        CpiContext::new_with_signer(
            context.accounts.token_program.address(),
            TransferChecked {
                from: context.accounts.fee_vault.to_cpi_handle_mut(),
                mint: context.accounts.quote_mint.to_cpi_handle(),
                to: context.accounts.authority_quote_account.to_cpi_handle_mut(),
                authority: context.accounts.market.cpi_handle(),
            },
            signer_seeds,
        ),
        fee_balance,
        quote_decimals,
    )?;
    context.accounts.market.reacquire_borrow_mut()?;

    Ok(())
}

#[derive(Accounts)]
pub struct WithdrawFeesAccountConstraints {
    #[account(mut)]
    pub market: BorshAccount<Market>,

    // Boxed to keep the struct under the BPF stack limit (see PlaceOrderAccountConstraints).
    #[account(mut, address = market.fee_vault @ ErrorCode::InvalidFeeVault)]
    pub fee_vault: Box<InterfaceAccount<TokenAccount>>,

    #[account(mut)]
    pub authority_quote_account: Box<InterfaceAccount<TokenAccount>>,

    pub quote_mint: Box<InterfaceAccount<Mint>>,

    pub authority: Signer,

    pub token_program: Interface<'static, TokenInterface>,
}
