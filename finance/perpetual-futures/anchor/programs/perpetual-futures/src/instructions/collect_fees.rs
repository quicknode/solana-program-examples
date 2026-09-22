use anchor_lang::prelude::*;
use anchor_spl::{
    associated_token::AssociatedToken,
    token_interface::{transfer_checked, Mint, TokenAccount, TokenInterface, TransferChecked},
};

use crate::constants::{POOL_SEED, VAULT_SEED};
use crate::errors::PerpError;
use crate::state::Pool;

pub fn handle_collect_fees(context: &mut Context<CollectFeesAccountConstraints>) -> Result<()> {
    let pool = &mut context.accounts.pool;
    let amount = pool.protocol_fees;
    require!(amount > 0, PerpError::NothingToClaim);

    // Effects before interaction: zero the balance, then transfer.
    pool.protocol_fees = 0;

    // The pool signs the CPI below with its own seeds. Copy them out first: a
    // data account holds a live borrow on its buffer, which the runtime
    // rejects when the CPI borrows the same account, so the borrow is
    // released around the CPI and taken back after. Releasing commits the
    // writes above; reacquiring re-reads the account.
    let collateral_mint_key = pool.collateral_mint;
    let oracle_feed_key = pool.oracle_feed;
    let pool_bump = [pool.bump];
    let pool_seeds: &[&[u8]] = &[
        POOL_SEED,
        collateral_mint_key.as_ref(),
        oracle_feed_key.as_ref(),
        &pool_bump,
    ];
    context.accounts.pool.release_borrow()?;
    transfer_checked(
        CpiContext::new_with_signer(
            context.accounts.token_program.address(),
            TransferChecked {
                from: context.accounts.custody_vault.to_cpi_handle_mut(),
                mint: context.accounts.collateral_mint.to_cpi_handle(),
                to: context.accounts.authority_collateral.to_cpi_handle_mut(),
                authority: context.accounts.pool.to_cpi_handle(),
            },
            &[pool_seeds],
        ),
        amount,
        context.accounts.collateral_mint.decimals(),
    )?;
    context.accounts.pool.reacquire_borrow_mut()?;

    Ok(())
}

#[derive(Accounts)]
pub struct CollectFeesAccountConstraints {
    #[account(mut, address = pool.authority)]
    pub authority: Signer,

    #[account(
        mut,
        seeds = [POOL_SEED, pool.collateral_mint.as_ref(), pool.oracle_feed.as_ref()],
        bump = pool.bump,
    )]
    pub pool: Box<BorshAccount<Pool>>,

    #[account(address = pool.collateral_mint)]
    pub collateral_mint: Box<InterfaceAccount<Mint>>,

    #[account(
        mut,
        seeds = [VAULT_SEED, pool.address().as_ref()],
        bump,
        address = pool.custody_vault,
    )]
    pub custody_vault: Box<InterfaceAccount<TokenAccount>>,

    #[account(
        init_if_needed,
        payer = authority,
        associated_token::mint = collateral_mint,
        associated_token::authority = authority,
        associated_token::token_program = token_program,
    )]
    pub authority_collateral: Box<InterfaceAccount<TokenAccount>>,

    pub token_program: Interface<'static, TokenInterface>,
    pub associated_token_program: Program<AssociatedToken>,
    pub system_program: Program<System>,
}
