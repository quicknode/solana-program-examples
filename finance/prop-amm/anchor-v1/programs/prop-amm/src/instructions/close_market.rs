use anchor_lang::prelude::*;
use anchor_spl::token_interface::{close_account, CloseAccount, TokenAccount, TokenInterface};

use crate::constants::{BASE_VAULT_SEED, MARKET_SEED, QUOTE_VAULT_SEED};
use crate::errors::PropAmmError;
use crate::state::Market;

/// The operator shuts the market down and takes back the rent it paid for the
/// market account and its two vaults.
///
/// Only the operator can close, through the same `has_one = operator`
/// constraint that guards `withdraw_inventory`. Both vaults must be empty: the
/// tokens in them are the operator's inventory, and closing a token account
/// that still holds tokens fails, so the handler refuses with
/// `InventoryNotEmpty` and the operator withdraws first. That includes any
/// tokens someone transferred straight into a vault; they are inventory like
/// the rest, and `withdraw_inventory` pays them out. The market account then
/// closes through its `close = operator` constraint, so all three rents go
/// back to the operator.
pub fn handle_close_market(context: Context<CloseMarketAccountConstraints>) -> Result<()> {
    require!(
        context.accounts.base_vault.amount == 0,
        PropAmmError::InventoryNotEmpty
    );
    require!(
        context.accounts.quote_vault.amount == 0,
        PropAmmError::InventoryNotEmpty
    );

    // The market owns both vaults and signs their closure with its own seeds.
    let market = &context.accounts.market;
    let market_bump = [market.bump];
    let market_seeds: &[&[u8]] = &[
        MARKET_SEED,
        market.base_mint.as_ref(),
        market.quote_mint.as_ref(),
        &market_bump,
    ];

    close_account(CpiContext::new_with_signer(
        context.accounts.token_program.key(),
        CloseAccount {
            account: context.accounts.base_vault.to_account_info(),
            destination: context.accounts.operator.to_account_info(),
            authority: context.accounts.market.to_account_info(),
        },
        &[market_seeds],
    ))?;

    close_account(CpiContext::new_with_signer(
        context.accounts.token_program.key(),
        CloseAccount {
            account: context.accounts.quote_vault.to_account_info(),
            destination: context.accounts.operator.to_account_info(),
            authority: context.accounts.market.to_account_info(),
        },
        &[market_seeds],
    ))?;

    Ok(())
}

#[derive(Accounts)]
pub struct CloseMarketAccountConstraints<'info> {
    #[account(mut)]
    pub operator: Signer<'info>,

    #[account(
        mut,
        seeds = [MARKET_SEED, market.base_mint.as_ref(), market.quote_mint.as_ref()],
        bump = market.bump,
        has_one = operator,
        has_one = base_vault,
        has_one = quote_vault,
        close = operator,
    )]
    pub market: Box<Account<'info, Market>>,

    #[account(
        mut,
        seeds = [BASE_VAULT_SEED, market.key().as_ref()],
        bump,
    )]
    pub base_vault: Box<InterfaceAccount<'info, TokenAccount>>,

    #[account(
        mut,
        seeds = [QUOTE_VAULT_SEED, market.key().as_ref()],
        bump,
    )]
    pub quote_vault: Box<InterfaceAccount<'info, TokenAccount>>,

    pub token_program: Interface<'info, TokenInterface>,
}
