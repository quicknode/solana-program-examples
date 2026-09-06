use anchor_lang::prelude::*;
use anchor_spl::{
    associated_token::AssociatedToken,
    token_interface::{Mint, TokenAccount, TokenInterface},
};

use crate::constants::{AUTHORITY_SEED, MARKET_SEED, QUOTE_VAULT_SEED};
use crate::errors::OptionsError;
use crate::instructions::shared::{check_custody, transfer_from_vault};
use crate::state::Market;

/// The admin sweeps the fees the venue has earned on premiums. `fees_owed`
/// is the only part of the quote vault the admin can reach: the collateral
/// and strike payments beside it are locked to their writers and holders.
pub fn handle_collect_fees(context: Context<CollectFeesAccountConstraints>) -> Result<()> {
    let market = &mut context.accounts.market;
    let amount = market.fees_owed;
    require!(amount > 0, OptionsError::NothingToCollect);

    // Effects before the transfer: zero the balance, then pay it out.
    market.fees_owed = 0;
    let quote_after = context
        .accounts
        .quote_vault
        .amount
        .checked_sub(amount)
        .ok_or(OptionsError::CustodyInvariantViolated)?;
    check_custody(
        market,
        context.accounts.underlying_vault.amount,
        quote_after,
    )?;

    transfer_from_vault(
        &context.accounts.token_program,
        &mut context.accounts.quote_vault,
        &context.accounts.quote_mint,
        &mut context.accounts.admin_quote,
        &context.accounts.market_authority,
        market,
        amount,
    )
}

#[derive(Accounts)]
pub struct CollectFeesAccountConstraints<'info> {
    #[account(mut, address = market.admin)]
    pub admin: Signer<'info>,

    #[account(
        mut,
        seeds = [MARKET_SEED, market.underlying_mint.as_ref(), market.quote_mint.as_ref()],
        bump = market.bump,
    )]
    pub market: Box<Account<'info, Market>>,

    /// CHECK: PDA authority over both vaults; holds no data, only signs.
    #[account(
        seeds = [AUTHORITY_SEED, market.key().as_ref()],
        bump = market.authority_bump,
    )]
    pub market_authority: UncheckedAccount<'info>,

    #[account(address = market.quote_mint)]
    pub quote_mint: Box<InterfaceAccount<'info, Mint>>,

    // Read only, for the custody check: the invariant covers both vaults.
    #[account(address = market.underlying_vault)]
    pub underlying_vault: Box<InterfaceAccount<'info, TokenAccount>>,

    #[account(
        mut,
        seeds = [QUOTE_VAULT_SEED, market.key().as_ref()],
        bump,
        address = market.quote_vault,
    )]
    pub quote_vault: Box<InterfaceAccount<'info, TokenAccount>>,

    #[account(
        init_if_needed,
        payer = admin,
        associated_token::mint = quote_mint,
        associated_token::authority = admin,
        associated_token::token_program = token_program,
    )]
    pub admin_quote: Box<InterfaceAccount<'info, TokenAccount>>,

    pub token_program: Interface<'info, TokenInterface>,
    pub associated_token_program: Program<'info, AssociatedToken>,
    pub system_program: Program<'info, System>,
}
