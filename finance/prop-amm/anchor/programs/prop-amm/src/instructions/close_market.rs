use anchor_lang::prelude::*;
use anchor_spl::token_interface::{close_account, CloseAccount, TokenAccount, TokenInterface};

use crate::constants::{BASE_VAULT_SEED, MARKET_SEED, QUOTE_VAULT_SEED};
use crate::errors::PropAmmError;
use crate::state::Market;

/// The operator shuts the market down and takes back the rent it paid for the
/// market account and its two vaults.
///
/// Only the operator can close, through the same `address = market.operator`
/// constraint that guards `withdraw_inventory`. Both vaults must be empty: the
/// tokens in them are the operator's inventory, and closing a token account
/// that still holds tokens fails, so the handler refuses with
/// `InventoryNotEmpty` and the operator withdraws first. That includes any
/// tokens someone transferred straight into a vault; they are inventory like
/// the rest, and `withdraw_inventory` pays them out. The market account then
/// closes through its `close = operator` constraint, so all three rents go
/// back to the operator.
pub fn handle_close_market(context: &mut Context<CloseMarketAccountConstraints>) -> Result<()> {
    require!(
        context.accounts.base_vault.amount() == 0,
        PropAmmError::InventoryNotEmpty
    );
    require!(
        context.accounts.quote_vault.amount() == 0,
        PropAmmError::InventoryNotEmpty
    );

    // Read the seeds before releasing the market's borrow below.
    let base_mint = context.accounts.market.base_mint;
    let quote_mint = context.accounts.market.quote_mint;
    let market_bump = [context.accounts.market.bump];
    let market_seeds: &[&[u8]] = &[
        MARKET_SEED,
        base_mint.as_ref(),
        quote_mint.as_ref(),
        &market_bump,
    ];

    // The market signs both CPIs below. It is a writable data account holding
    // a live borrow on its buffer, so release it across the CPIs: the runtime
    // rejects a CPI that borrows an account we still hold. Take it back after.
    context.accounts.market.release_borrow()?;
    let market_view = *context.accounts.market.account();

    close_account(CpiContext::new_with_signer(
        context.accounts.token_program.address(),
        CloseAccount {
            account: context.accounts.base_vault.to_cpi_handle_mut(),
            destination: context.accounts.operator.cpi_handle_mut(),
            authority: CpiHandle::readonly(&market_view),
        },
        &[market_seeds],
    ))?;

    close_account(CpiContext::new_with_signer(
        context.accounts.token_program.address(),
        CloseAccount {
            account: context.accounts.quote_vault.to_cpi_handle_mut(),
            destination: context.accounts.operator.cpi_handle_mut(),
            authority: CpiHandle::readonly(&market_view),
        },
        &[market_seeds],
    ))?;

    // Take the borrow back before the derive's exit path closes the market.
    context.accounts.market.reacquire_borrow_mut()?;

    Ok(())
}

#[derive(Accounts)]
pub struct CloseMarketAccountConstraints {
    // `address = market.operator` is the access control, as on
    // `withdraw_inventory`: only the firm that opened the market closes it.
    #[account(mut, address = market.operator)]
    pub operator: Signer,

    #[account(
        mut,
        seeds = [MARKET_SEED, market.base_mint.as_ref(), market.quote_mint.as_ref()],
        bump = market.bump,
        close = operator,
    )]
    pub market: Box<BorshAccount<Market>>,

    #[account(
        mut,
        seeds = [BASE_VAULT_SEED, market.address().as_ref()],
        bump,
        address = market.base_vault,
    )]
    pub base_vault: Box<InterfaceAccount<TokenAccount>>,

    #[account(
        mut,
        seeds = [QUOTE_VAULT_SEED, market.address().as_ref()],
        bump,
        address = market.quote_vault,
    )]
    pub quote_vault: Box<InterfaceAccount<TokenAccount>>,

    pub token_program: Interface<'static, TokenInterface>,
}
