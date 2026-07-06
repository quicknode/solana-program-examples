use quasar_lang::prelude::*;
use quasar_spl::prelude::*;

use crate::errors::OrderBookError;
use crate::state::{snapshot_market_user, Market, MarketUser, MARKET_SEED};

#[derive(Accounts)]
pub struct SettleFundsAccountConstraints {
    pub owner: Signer,

    // `has_one` binds these vaults/mints to the addresses stored on the Market
    // PDA. Without them a caller could substitute the fee_vault (same mint +
    // authority as quote_vault) for `quote_vault` and drain accumulated taker
    // fees, since transfer_checked only verifies mint + authority on the
    // source account, not its identity.
    #[account(
        has_one(base_vault) @ OrderBookError::InvalidBaseVault,
        has_one(quote_vault) @ OrderBookError::InvalidQuoteVault,
        has_one(base_mint) @ OrderBookError::InvalidBaseMint,
        has_one(quote_mint) @ OrderBookError::InvalidQuoteMint,
    )]
    pub market: Account<Market>,

    #[account(mut, address = MarketUser::seeds(market.address(), owner.address()))]
    pub market_user: Account<MarketUser>,

    #[account(mut)]
    pub base_vault: Account<Token>,
    #[account(mut)]
    pub quote_vault: Account<Token>,
    #[account(mut)]
    pub user_base_account: Account<Token>,
    #[account(mut)]
    pub user_quote_account: Account<Token>,

    pub base_mint: Account<Mint>,
    pub quote_mint: Account<Mint>,

    pub token_program: Program<TokenProgram>,
}

#[inline(always)]
pub fn handle_settle_funds(
    accounts: &mut SettleFundsAccountConstraints,
) -> Result<(), ProgramError> {
    // Snapshot the amounts owed, then zero the counters BEFORE the token
    // transfers (checks-effects-interactions): updating state first makes a
    // re-entry double-withdraw impossible even if a token hook ever gained a
    // path back into this program.
    let mut market_user = snapshot_market_user(&accounts.market_user);
    let base_amount = market_user.unsettled_base;
    let quote_amount = market_user.unsettled_quote;
    market_user.unsettled_base = 0;
    market_user.unsettled_quote = 0;
    accounts.market_user.set_inner(market_user);

    // Seeds to sign as the market PDA (the authority of both vaults).
    let base_mint = accounts.market.base_mint;
    let quote_mint = accounts.market.quote_mint;
    let bump = [accounts.market.bump];
    let seeds = [
        Seed::from(MARKET_SEED),
        Seed::from(base_mint.as_ref()),
        Seed::from(quote_mint.as_ref()),
        Seed::from(bump.as_ref()),
    ];

    if base_amount > 0 {
        accounts
            .token_program
            .transfer_checked(
                &accounts.base_vault,
                &accounts.base_mint,
                &accounts.user_base_account,
                &accounts.market,
                base_amount,
                accounts.base_mint.decimals,
            )
            .invoke_signed(&seeds)?;
    }

    if quote_amount > 0 {
        accounts
            .token_program
            .transfer_checked(
                &accounts.quote_vault,
                &accounts.quote_mint,
                &accounts.user_quote_account,
                &accounts.market,
                quote_amount,
                accounts.quote_mint.decimals,
            )
            .invoke_signed(&seeds)?;
    }

    Ok(())
}
