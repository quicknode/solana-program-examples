use anchor_lang::prelude::*;
use anchor_spl::{
    associated_token::AssociatedToken,
    token_interface::{mint_to, Mint, MintTo, TokenAccount, TokenInterface},
};

use crate::error::VaultError;
use crate::state::Strategy;

const SECONDS_PER_YEAR: u64 = 31_536_000;

#[derive(Accounts)]
pub struct CollectFeesAccountConstraints<'info> {
    /// CHECK: manager is stored in strategy; we only read their pubkey for derivation
    pub manager: UncheckedAccount<'info>,

    #[account(
        mut,
        has_one = manager,
        seeds = [b"strategy", strategy.manager.as_ref()],
        bump = strategy.bump
    )]
    pub strategy: Account<'info, Strategy>,

    #[account(
        mut,
        seeds = [b"share_mint", strategy.key().as_ref()],
        bump
    )]
    pub share_mint: InterfaceAccount<'info, Mint>,

    /// Manager's share token account — receives fee shares
    #[account(
        init_if_needed,
        payer = payer,
        associated_token::mint = share_mint,
        associated_token::authority = manager,
        associated_token::token_program = token_program
    )]
    pub manager_share_account: InterfaceAccount<'info, TokenAccount>,

    #[account(mut)]
    pub payer: Signer<'info>,

    pub associated_token_program: Program<'info, AssociatedToken>,
    pub token_program: Interface<'info, TokenInterface>,
    pub system_program: Program<'info, System>,
}

pub fn handle_collect_fees(context: Context<CollectFeesAccountConstraints>) -> Result<()> {
    let clock = Clock::get()?;
    let current_ts = clock.unix_timestamp;
    let last_ts = context.accounts.strategy.last_fee_accrual_timestamp;

    require!(current_ts > last_ts, VaultError::NoTimeElapsed);

    let elapsed_seconds = (current_ts - last_ts) as u64;
    let total_shares = context.accounts.strategy.total_shares;
    let fee_bps = context.accounts.strategy.fee_bps;
    let manager_key = context.accounts.strategy.manager;
    let strategy_bump = context.accounts.strategy.bump;

    // fee_shares = total_shares * fee_bps * elapsed / (10_000 * SECONDS_PER_YEAR)
    let denominator = (10_000u128)
        .checked_mul(SECONDS_PER_YEAR as u128)
        .ok_or(VaultError::MathOverflow)?;

    let fee_shares: u64 = (total_shares as u128)
        .checked_mul(fee_bps as u128)
        .ok_or(VaultError::MathOverflow)?
        .checked_mul(elapsed_seconds as u128)
        .ok_or(VaultError::MathOverflow)?
        .checked_div(denominator)
        .ok_or(VaultError::MathOverflow)? as u64;

    // Update timestamp even if fee_shares rounds to zero
    context.accounts.strategy.last_fee_accrual_timestamp = current_ts;

    if fee_shares == 0 {
        return Ok(());
    }

    // Checks-effects-interactions: update total_shares before CPI
    context.accounts.strategy.total_shares = total_shares
        .checked_add(fee_shares)
        .ok_or(VaultError::MathOverflow)?;

    // Mint fee shares to manager — strategy PDA signs
    let signer_seeds: &[&[&[u8]]] = &[&[b"strategy", manager_key.as_ref(), &[strategy_bump]]];

    let mint_accounts = MintTo {
        mint: context.accounts.share_mint.to_account_info(),
        to: context.accounts.manager_share_account.to_account_info(),
        authority: context.accounts.strategy.to_account_info(),
    };
    let cpi_ctx = CpiContext::new_with_signer(
        context.accounts.token_program.key(),
        mint_accounts,
        signer_seeds,
    );
    mint_to(cpi_ctx, fee_shares)?;

    Ok(())
}
