use anchor_lang::prelude::*;
use anchor_spl::{
    associated_token::AssociatedToken,
    token_interface::{transfer_checked, Mint, TokenAccount, TokenInterface, TransferChecked},
};

use crate::constants::{AUTHORITY_SEED, POOL_SEED, VAULT_SEED};
use crate::errors::PerpError;
use crate::state::Pool;

pub fn handle_collect_fees(context: &mut Context<CollectFeesAccountConstraints>) -> Result<()> {
    let pool = &mut context.accounts.pool;
    let amount = pool.protocol_fees;
    require!(amount > 0, PerpError::NothingToClaim);

    // Effects before interaction: zero the balance, then transfer.
    pool.protocol_fees = 0;

    let pool_key = pool.address();
    let authority_seeds: &[&[u8]] = &[AUTHORITY_SEED, pool_key.as_ref(), &[pool.authority_bump]];
    transfer_checked(
        CpiContext::new_with_signer(
            context.accounts.token_program.address(),
            TransferChecked {
                from: context.accounts.custody_vault.cpi_handle_mut(),
                mint: context.accounts.collateral_mint.cpi_handle(),
                to: context.accounts.authority_collateral.cpi_handle_mut(),
                authority: context.accounts.pool_authority.cpi_handle(),
            },
            &[authority_seeds],
        ),
        amount,
        context.accounts.collateral_mint.decimals(),
    )?;

    Ok(())
}

#[derive(Accounts)]
pub struct CollectFeesAccountConstraints {
    #[account(mut)]
    pub authority: Signer,

    #[account(
        mut,
        seeds = [POOL_SEED, pool.collateral_mint.as_ref(), pool.oracle_feed.as_ref()],
        bump = pool.bump,
        has_one = authority,
        has_one = collateral_mint,
        has_one = custody_vault,
    )]
    pub pool: Box<BorshAccount<Pool>>,

    /// CHECK: PDA authority over the vault.
    #[account(
        seeds = [AUTHORITY_SEED, pool.address().as_ref()],
        bump = pool.authority_bump,
    )]
    pub pool_authority: UncheckedAccount,

    pub collateral_mint: Box<InterfaceAccount<Mint>>,

    #[account(
        mut,
        seeds = [VAULT_SEED, pool.address().as_ref()],
        bump,
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
