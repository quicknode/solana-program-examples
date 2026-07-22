use quasar_lang::cpi::Seed;
use quasar_lang::prelude::*;
use quasar_spl::prelude::*;

use crate::errors::OrderBookError;
use crate::state::{Market, MARKET_SEED};

/// Drain the market's accumulated taker fees into the authority's token
/// account. Authority-only - arbitrary callers must not be able to siphon the
/// fee vault. Transfers the current balance of the fee vault in full.
#[derive(Accounts)]
pub struct WithdrawFeesAccountConstraints {
    #[account(has_one(fee_vault) @ OrderBookError::InvalidFeeVault)]
    pub market: Account<Market>,

    #[account(mut)]
    pub fee_vault: Account<Token>,

    #[account(mut)]
    pub authority_quote_account: Account<Token>,

    pub quote_mint: Account<Mint>,

    pub authority: Signer,

    pub token_program: Program<TokenProgram>,
}

#[inline(always)]
pub fn handle_withdraw_fees(
    accounts: &mut WithdrawFeesAccountConstraints,
) -> Result<(), ProgramError> {
    require_keys_eq!(
        *accounts.authority.address(),
        accounts.market.authority,
        OrderBookError::NotMarketAuthority
    );

    let fee_balance = accounts.fee_vault.amount();
    if fee_balance == 0 {
        // Nothing to do - exit quietly rather than failing, so this
        // instruction is safe to call on a cron/heartbeat even when there
        // haven't been any fills since the last run.
        return Ok(());
    }

    let base_mint = accounts.market.base_mint;
    let quote_mint = accounts.market.quote_mint;
    let bump = [accounts.market.bump];
    let seeds = [
        Seed::from(MARKET_SEED),
        Seed::from(base_mint.as_ref()),
        Seed::from(quote_mint.as_ref()),
        Seed::from(bump.as_ref()),
    ];

    accounts
        .token_program
        .transfer_checked(
            &accounts.fee_vault,
            &accounts.quote_mint,
            &accounts.authority_quote_account,
            &accounts.market,
            fee_balance,
            accounts.quote_mint.decimals,
        )
        .invoke_signed(&seeds)?;

    Ok(())
}
