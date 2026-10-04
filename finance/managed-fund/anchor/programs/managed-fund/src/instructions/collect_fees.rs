use anchor_lang::prelude::*;
use anchor_spl::{
    associated_token::AssociatedToken,
    token_interface::{mint_to, Mint, MintTo, TokenAccount, TokenInterface},
};

use crate::error::FundError;
use crate::state::Fund;

const SECONDS_PER_YEAR: u64 = 31_536_000;

#[derive(Accounts)]
pub struct CollectFeesAccountConstraints {
    /// CHECK: manager is stored in fund; we only read their pubkey for derivation
    #[account(address = fund.manager)]
    pub manager: UncheckedAccount,

    #[account(
        mut,
        seeds = [b"fund", fund.index.to_le_bytes()],
        bump = fund.bump,
    )]
    pub fund: BorshAccount<Fund>,

    #[account(
        mut,
        seeds = [b"share_mint", fund.address().as_ref()],
        bump
    )]
    pub share_mint: InterfaceAccount<Mint>,

    /// Manager's share token account - receives fee shares
    #[account(
        init_if_needed,
        payer = payer,
        associated_token::mint = share_mint,
        associated_token::authority = manager,
        associated_token::token_program = token_program
    )]
    pub manager_share_account: InterfaceAccount<TokenAccount>,

    #[account(mut)]
    pub payer: Signer,

    pub associated_token_program: Program<AssociatedToken>,
    pub token_program: Interface<'static, TokenInterface>,
    pub system_program: Program<System>,
}

pub fn handle_collect_fees(context: &mut Context<CollectFeesAccountConstraints>) -> Result<()> {
    let clock = Clock::get()?;
    let current_ts = clock.unix_timestamp;
    let last_ts = context.accounts.fund.last_fee_accrual_timestamp;

    require!(current_ts > last_ts, FundError::NoTimeElapsed);

    let elapsed_seconds = (current_ts - last_ts) as u64;
    let total_shares = context.accounts.fund.total_shares;
    let fee_bps = context.accounts.fund.fee_bps;
    let fund_index = context.accounts.fund.index;
    let fund_bump = context.accounts.fund.bump;

    // fee_shares = total_shares * fee_bps * elapsed / (10_000 * SECONDS_PER_YEAR)
    let denominator = (10_000u128)
        .checked_mul(SECONDS_PER_YEAR as u128)
        .ok_or(FundError::MathOverflow)?;

    let fee_shares: u64 = (total_shares as u128)
        .checked_mul(fee_bps as u128)
        .ok_or(FundError::MathOverflow)?
        .checked_mul(elapsed_seconds as u128)
        .ok_or(FundError::MathOverflow)?
        .checked_div(denominator)
        .ok_or(FundError::MathOverflow)? as u64;

    // Update timestamp even if fee_shares rounds to zero
    context.accounts.fund.last_fee_accrual_timestamp = current_ts;

    if fee_shares == 0 {
        return Ok(());
    }

    // Add the fee shares to the recorded supply
    context.accounts.fund.total_shares = total_shares
        .checked_add(fee_shares)
        .ok_or(FundError::MathOverflow)?;

    // Mint fee shares to manager - fund PDA signs
    let index_bytes = fund_index.to_le_bytes();
    let signer_seeds: &[&[&[u8]]] = &[&[b"fund", index_bytes.as_ref(), &[fund_bump]]];

    // `fund` signs the CPI(s) below. It is a data account holding a live
    // borrow on its buffer, which the runtime would reject when the CPI borrows
    // the same account, so hand the borrow back for the duration.
    context.accounts.fund.release_borrow()?;

    let mint_accounts = MintTo {
        mint: context.accounts.share_mint.to_cpi_handle_mut(),
        to: context.accounts.manager_share_account.cpi_handle_mut(),
        authority: context.accounts.fund.to_cpi_handle(),
    };
    let cpi_ctx = CpiContext::new_with_signer(
        context.accounts.token_program.address(),
        mint_accounts,
        signer_seeds,
    );
    mint_to(cpi_ctx, fee_shares)?;

    context.accounts.fund.reacquire_borrow_mut()?;

    Ok(())
}
